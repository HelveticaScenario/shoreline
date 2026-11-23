//! The open audio streams, and the one path every open takes: resolve the
//! devices and settings against the device cache (`stream_resolve`), then
//! build the streams.

use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::StreamTrait;
use napi::Result;
use napi_derive::napi;
use parking_lot::Mutex;

use crate::Synthesizer;
use crate::audio::{
    self, AudioState, Engine, EngineSlot, InputBufferWriter, create_audio_channels,
    create_input_ring_buffer, make_input_stream, make_stream,
};
use crate::midi::MidiInputManager;
use crate::stream_health::{StreamHealth, StreamWatchdog};
use crate::stream_resolve::{OpenRequest, StreamPlan, resolve};

/// Fewest frames per internal block: below this, each module's per-block work
/// (resolving inputs, dispatch) costs more than a shorter callback saves.
const MIN_BLOCK_SIZE: usize = 64;
/// Most frames per internal block, bounding each block buffer's memory.
const MAX_BLOCK_SIZE: usize = 1024;
/// Frames per internal block when the host picks the device buffer size.
const DEFAULT_BLOCK_SIZE: usize = 256;

/// How long a stream change waits for a dropped output stream to return its
/// engine before leaving the engine to be adopted on the next poll.
const ENGINE_RETURN_WAIT: Duration = Duration::from_millis(250);

/// The audio callback renders in internal blocks of this many frames: one per
/// callback when it matches the device buffer (`buffer_size` frames, None for
/// the host's default). A callback shorter than a block renders only the
/// frames it emits and the next continues the block, so a block longer than
/// the device buffer adds no latency.
fn block_size_for(buffer_size: Option<u32>) -> usize {
    buffer_size
        .map_or(DEFAULT_BLOCK_SIZE, |frames| frames as usize)
        .clamp(MIN_BLOCK_SIZE, MAX_BLOCK_SIZE)
}

/// The streams open on the output device and, optionally, an input device,
/// with the settings they run at. A stream open replaces them whole.
pub(crate) struct Streams {
    output: cpal::Stream,
    input: Option<cpal::Stream>,
    pub(crate) output_health: Arc<StreamHealth>,
    pub(crate) input_health: Arc<StreamHealth>,
    pub(crate) watchdog: StreamWatchdog,
    /// Where the output stream's callback finds the engine, and where the
    /// engine goes back when that stream is dropped.
    engine_slot: EngineSlot,
    /// The slot of a dropped output stream whose callback outlived the drop
    /// (cpal's CoreAudio notification thread can hold it a moment longer).
    /// Its engine is moved to `engine_slot` once the callback lets it go.
    returning_engine: Option<EngineSlot>,
    pub(crate) host_id: String,
    pub(crate) output_device_id: String,
    pub(crate) input_device_id: Option<String>,
    pub(crate) sample_rate: u32,
    /// The buffer size in frames; `None` is the host's default.
    pub(crate) buffer_size: Option<u32>,
    pub(crate) channels: u16,
    pub(crate) input_channels: u16,
}

impl Streams {
    /// Open and start the streams `plan` describes, playing the engine
    /// `state` drives, which the output callback takes from `engine_slot`
    /// once it is there. An input that will not start leaves the input off.
    fn open(plan: StreamPlan, state: &AudioState, engine_slot: EngineSlot) -> Result<Self> {
        let channels = plan.output_config.channels();
        let buffer_size = plan
            .buffer_size
            .map(cpal::BufferSize::Fixed)
            .unwrap_or(cpal::BufferSize::Default);
        let stream_config = cpal::StreamConfig {
            channels,
            sample_rate: plan.sample_rate,
            buffer_size,
        };

        let input_channels = plan
            .input
            .as_ref()
            .map_or(0, |(_, config, _)| config.channels() as usize);
        let (input_writer, input_reader) = create_input_ring_buffer(input_channels);
        let output_health = Arc::new(StreamHealth::default());
        let input_health = Arc::new(StreamHealth::default());

        let output = build_output_stream(
            &plan.output_device,
            &stream_config,
            plan.output_config.sample_format(),
            state.stream_endpoints(engine_slot.clone(), input_reader, output_health.clone()),
        )?;
        output.play().map_err(|e| {
            napi::Error::from_reason(format!("Failed to start output stream: {}", e))
        })?;

        let (input, input_device_id, input_channels) =
            match plan.input.and_then(|(device, config, id)| {
                let input_config = cpal::StreamConfig {
                    channels: config.channels(),
                    sample_rate: plan.sample_rate,
                    buffer_size,
                };
                let stream = build_input_stream(
                    &device,
                    &input_config,
                    config.sample_format(),
                    input_writer,
                    input_health.clone(),
                )
                .inspect_err(|e| eprintln!("Failed to create input stream: {}", e))
                .ok()?;
                stream
                    .play()
                    .inspect_err(|e| eprintln!("Failed to start input stream: {}", e))
                    .ok()?;
                println!(
                    "Audio input: {} ({} channels, {} Hz)",
                    id, input_channels, plan.sample_rate
                );
                Some((stream, id))
            }) {
                Some((stream, id)) => (Some(stream), Some(id), input_channels as u16),
                None => (None, None, 0),
            };

        Ok(Self {
            output,
            input,
            output_health,
            input_health,
            watchdog: StreamWatchdog::new(Instant::now()),
            engine_slot,
            returning_engine: None,
            host_id: plan.host_id,
            output_device_id: plan.output_device_id,
            input_device_id,
            sample_rate: plan.sample_rate,
            buffer_size: plan.buffer_size,
            channels,
            input_channels,
        })
    }

    /// The devices these streams hold open: an ended stream holds none.
    pub(crate) fn open_device_ids(&self) -> impl Iterator<Item = &str> {
        let output = (!self.watchdog.has_died()).then_some(self.output_device_id.as_str());
        let input = self
            .input_device_id
            .as_deref()
            .filter(|_| !self.input_health.has_ended());
        output.into_iter().chain(input)
    }

    /// Move the engine a dropped output stream returned to this stream's
    /// slot, waiting up to `wait` for it. False when it is still on its way.
    pub(crate) fn adopt_returning_engine(&mut self, wait: Duration) -> bool {
        let Some(slot) = self.returning_engine.take() else {
            return true;
        };
        let deadline = Instant::now() + wait;
        loop {
            if let Some(mut engine) = slot.lock().take() {
                engine.resume_on_new_stream();
                *self.engine_slot.lock() = Some(engine);
                return true;
            }
            if Instant::now() >= deadline {
                self.returning_engine = Some(slot);
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Build output stream with the appropriate sample format
fn build_output_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    endpoints: audio::AudioThreadEndpoints,
) -> Result<cpal::Stream> {
    match sample_format {
        cpal::SampleFormat::I8 => make_stream::<i8>(device, config, endpoints),
        cpal::SampleFormat::I16 => make_stream::<i16>(device, config, endpoints),
        cpal::SampleFormat::I32 => make_stream::<i32>(device, config, endpoints),
        cpal::SampleFormat::F32 => make_stream::<f32>(device, config, endpoints),
        _ => Err(napi::Error::from_reason(format!(
            "Unsupported output sample format: {:?}",
            sample_format
        ))),
    }
}

/// Build input stream with the appropriate sample format
fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    input_writer: InputBufferWriter,
    health: Arc<StreamHealth>,
) -> Result<cpal::Stream> {
    match sample_format {
        cpal::SampleFormat::I8 => make_input_stream::<i8>(device, config, input_writer, health),
        cpal::SampleFormat::I16 => make_input_stream::<i16>(device, config, input_writer, health),
        cpal::SampleFormat::I32 => make_input_stream::<i32>(device, config, input_writer, health),
        cpal::SampleFormat::F32 => make_input_stream::<f32>(device, config, input_writer, health),
        _ => Err(napi::Error::from_reason(format!(
            "Unsupported input sample format: {:?}",
            sample_format
        ))),
    }
}

/// A new engine at `sample_rate`, waiting in its slot for a stream, and the
/// state the main thread drives it through.
fn new_engine(
    sample_rate: u32,
    buffer_size: Option<u32>,
    channels: u16,
    midi_manager: Arc<MidiInputManager>,
) -> (Arc<AudioState>, EngineSlot) {
    let (command_tx, command_rx, error_tx, error_rx, garbage_tx, garbage_rx) =
        create_audio_channels();
    let block_size = block_size_for(buffer_size);
    let state = Arc::new(AudioState::new_with_channels(
        command_tx,
        error_rx,
        garbage_rx,
        sample_rate as f32,
        channels,
        midi_manager,
        block_size,
    ));
    let engine = Engine::new(
        command_rx,
        error_tx,
        garbage_tx,
        state.get_shared_state(),
        sample_rate as f32,
        block_size,
    );
    (state, Arc::new(Mutex::new(Some(Box::new(engine)))))
}

/// A new engine and the streams that play it, opened as `plan` says.
pub(crate) fn launch(
    plan: StreamPlan,
    midi_manager: Arc<MidiInputManager>,
) -> Result<(Arc<AudioState>, Streams)> {
    let (state, engine_slot) = new_engine(
        plan.sample_rate,
        plan.buffer_size,
        plan.output_config.channels(),
        midi_manager,
    );
    let streams = Streams::open(plan, &state, engine_slot)?;
    Ok((state, streams))
}

/// What replacing the streams did to the engine.
#[napi(object)]
pub struct StreamsChange {
    /// A fresh engine, stopped and empty, replaced the one holding the patch.
    pub engine_reset: bool,
    /// The file of a recording that ended with the old engine.
    pub recording_path: Option<String>,
}

impl Synthesizer {
    /// Resolve `request` and open its streams, replacing the running ones.
    pub(crate) fn open(&mut self, request: OpenRequest) -> Result<StreamsChange> {
        let plan = resolve(&self.device_cache, request)?;
        self.open_streams(plan)
    }

    /// Open the streams `plan` describes, replacing the running ones. At the
    /// engine's sample rate and buffer size the engine moves to the new
    /// streams with its patch, playback, recording and Link. Otherwise a
    /// fresh engine starts stopped and empty, with Link as it was, and a
    /// recording in progress ends with the old streams.
    pub(crate) fn open_streams(&mut self, plan: StreamPlan) -> Result<StreamsChange> {
        // An engine keeps playing on the new streams when they run at its
        // sample rate and buffer size, which its block size is built from.
        let keep_engine = plan.sample_rate == self.streams.sample_rate
            && plan.buffer_size == self.streams.buffer_size
            && !self.state.is_audio_thread_panicked();
        let (state, streams) = if keep_engine {
            let streams = Streams::open(plan, &self.state, Arc::new(Mutex::new(None)))?;
            (self.state.clone(), streams)
        } else {
            launch(plan, self.midi_manager.clone())?
        };
        let Streams {
            output,
            input,
            engine_slot: previous_slot,
            returning_engine,
            ..
        } = std::mem::replace(&mut self.streams, streams);
        // Dropping the old output stream puts its engine back in its slot.
        drop((output, input));
        self.fallback_warning = None;

        println!(
            "Audio streams recreated: output={} input={:?} {}Hz {}ch host={}",
            self.streams.output_device_id,
            self.streams.input_device_id,
            self.streams.sample_rate,
            self.streams.channels,
            self.streams.host_id
        );

        if keep_engine {
            self.state.set_channels(self.streams.channels);
            // The engine is in the slot of the stream just dropped, or still
            // on its way back from an earlier one, whose slot it returns to.
            self.streams.returning_engine = Some(returning_engine.unwrap_or(previous_slot));
            self.streams.adopt_returning_engine(ENGINE_RETURN_WAIT);
            return Ok(StreamsChange {
                engine_reset: false,
                recording_path: None,
            });
        }

        // An engine still on its way back from a dropped stream is dropped
        // when it arrives.
        drop(returning_engine);
        let bpm = self.state.transport_meter.read_bpm();
        let previous_state = std::mem::replace(&mut self.state, state);
        // The fresh AudioState starts with a zero profiling refcount, so reset
        // the process-global profiling flag to match. Otherwise switching audio
        // devices while profiling is enabled leaves the global stuck on with no
        // refcount able to clear it; the UI panel re-enables on its next open.
        modular_core::profiling::set_enabled(false);

        if previous_state.transport_meter.read_link_enabled()
            && let Err(error) = self.enable_link_at(bpm)
        {
            eprintln!("Link did not rejoin on the new streams: {}", error);
        }

        let recording_path = if std::mem::take(&mut self.is_recording) {
            match previous_state.stop_recording() {
                Ok(finished) => finished.map(|f| f.path.to_string_lossy().to_string()),
                Err(error) => {
                    eprintln!("Recording did not finish cleanly: {}", error);
                    None
                }
            }
        } else {
            None
        };
        Ok(StreamsChange {
            engine_reset: true,
            recording_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_size_follows_the_device_buffer_within_bounds() {
        assert_eq!(block_size_for(Some(128)), 128);
        assert_eq!(block_size_for(Some(512)), 512);
        assert_eq!(block_size_for(Some(32)), 64);
        assert_eq!(block_size_for(Some(4096)), 1024);
        assert_eq!(block_size_for(None), 256);
    }
}
