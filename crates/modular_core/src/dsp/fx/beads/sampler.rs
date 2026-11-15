//! `$granularSampler`: Beads' grain voices playing a WAV, loaded into each
//! voice's buffer instead of a live recording.

use std::collections::HashMap;
use std::sync::Arc;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::Wav;
use crate::dsp::utils::rng::{LcgRng, seed_base};
use crate::patch::Patch;
use crate::poly::{PORT_MAX_LANES, PolyOutput, PolySignal, PolySignalExt};
use crate::types::{PatchUpdateHandler, PolySignalFields, WavData};

use super::buffer::GrainBuffer;
use super::chain::Media;
use super::grains_chain::{GrainsChain, GrainsVoice};
use super::player::{GrainParams, GrainTrigger};
use super::queue::TriggerQueue;
use super::resample::Resampler;
use super::voice::VoiceControls;
use super::wav_load::cached_load;
use super::{FULL_SCALE_VOLTS, GranularQuality, grain_params, unit};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// A voice fully wet with no feedback: frozen, it records nothing.
const CONTROLS: VoiceControls = VoiceControls {
    feedback: 0.0,
    dry_wet: 1.0,
};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct GranularSamplerParams {
    #[input("the file to play; each of its channels gets its own voice")]
    wav: Wav,
    #[input("starts a grain", type = trig)]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "where grains start: 0 the start of the file, 5 its end",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    time: Option<PolySignal>,
    #[input(
        "grain length: 0 shortest, 5 longest; negative values play the grains backwards",
        type = control,
        default = 2.75,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    size: Option<PolySignal>,
    #[input(
        "grain envelope: 0 square, 1.67 sharp attack, 3.33 smooth, 5 reverse swell",
        type = control,
        default = 3.33,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    shape: Option<PolySignal>,
    #[input(
        "grain pitch; grains keep the file's pitch when `freq` equals `rootFreq`",
        type = pitch,
    )]
    #[deserr(default)]
    freq: Option<PolySignal>,
    #[input("pitch of the file, so `freq` plays in tune", type = pitch)]
    #[deserr(default)]
    root_freq: Option<PolySignal>,
    #[input(
        "how far each grain lands at a random stereo position: 0 centred, 5 anywhere",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    spread: Option<PolySignal>,
    #[input("sound character: how clean it sounds", default = "digital")]
    #[serde(default)]
    #[deserr(default)]
    quality: GranularQuality,
    /// Each voice's buffer loaded from `wav`, built on the main thread by
    /// `prepare_resources` and handed to the voices in `on_patch_update`.
    #[serde(skip)]
    #[deserr(skip)]
    #[schemars(skip)]
    loaded: Option<Loaded>,
}

/// Every voice's loaded buffer, each tagged with the identity of its
/// contents. A voice hands back the buffer it held in exchange, so a slot
/// is ready to hand over only while it is tagged `source`.
#[derive(Clone)]
struct Loaded {
    source: u64,
    buffers: Vec<GrainBuffer>,
}

impl crate::types::Connect for Loaded {
    fn apply_default_connections(&mut self) {}
    fn connect(&mut self, _patch: &Patch) {}
    fn collect_cables(&self, _sink: &mut Vec<crate::types::CableSource>) {}
    fn collect_lane_cables(&self, _sink: &mut Vec<crate::types::CableSource>) {}
    fn inject_index_ptr(&mut self, _ptr: *const std::cell::Cell<usize>) {}
    fn expand_lanes(&mut self, _expansion: &crate::types::LaneExpansion) {}
    fn mark_glides(
        &mut self,
        _old: Option<&Self>,
        _generation: u64,
        _ids: &dyn crate::types::PreviousIds,
    ) {
    }
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GranularSamplerOutputs {
    #[output(
        "output",
        "each lane's grains, in stereo",
        type = cv,
        default,
        range = (-5.0, 5.0),
    )]
    sample: PolyOutput,
}

#[derive(Default)]
struct GranularSamplerState {
    engine_sample_rate: f32,
    media: Media,
    chain: GrainsChain,
    resampler: Resampler,
    rng: LcgRng,
    /// Engine samples processed.
    sample: u64,
    pending: TriggerQueue,
}

fn sampler_lane_count(params: &GranularSamplerParams) -> usize {
    params
        .poly_signal_fields()
        .iter()
        .map(|p| p.lanes())
        .max()
        .unwrap_or(1)
        .max(params.wav.lanes())
        .clamp(1, PORT_MAX_LANES)
}

const GRANULAR_SAMPLER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Granular WAV player",
    description: "Plays grains from a WAV file, one per trigger, the way `$granular` does from \
                  a live input: turn a sample into a texture, a stretched drone or a pitched \
                  cloud. For grain-to-grain variation, modulate its settings with something \
                  sampled on the same trigger, such as `$sah`.",
    categories: &[ModuleCategory::Sampler],
    sections: &[],
    examples: &[DocExample {
        title: "Scattered pad texture",
        code: r#"
            const grains = $pulse('8hz')
            $granularSampler('/wavs/pad.wav', grains, {
              time: $sah($noise('white').range(0, 5), grains),
              spread: 2,
            }).out()
        "#,
    }],
};

#[module(
    name = "$granularSampler", stereo = stereo,
    docs = GRANULAR_SAMPLER_DOCS,
    lanes_derive = sampler_lane_count,
    has_init,
    patch_update,
    has_prepare_resources,
    args(wav, trigger)
)]
pub struct GranularSampler {
    outputs: GranularSamplerOutputs,
    state: GranularSamplerState,
    params: GranularSamplerParams,
    /// One voice per side of each lane: a lane per lane of the widest grain
    /// input, or per channel of a file with more than two.
    lane_state: Box<[GrainsVoice]>,
}

impl GranularSampler {
    /// Allocate every voice and configure for the initial quality. Main
    /// thread.
    fn init(&mut self, sample_rate: f32) {
        let seed = seed_base(self);
        self.state.chain.allocate();
        for voice in self.lane_state.iter_mut() {
            voice.allocate_playback();
        }
        let state = &mut self.state;
        state.engine_sample_rate = sample_rate;
        state.resampler.allocate(sample_rate);
        state.rng.seed(seed, 0);
        let media = self.params.quality.media();
        self.configure(media);
        // Voices play silence until `prepare_resources` loads the file.
        let (source, silence) = cached_load(None, media);
        for voice in self.lane_state.iter_mut() {
            let mut buffer = GrainBuffer::clone(&silence[0]);
            buffer.set_source(source);
            voice.voice.granular.buffer = buffer;
        }
    }

    fn configure(&mut self, media: Media) {
        let state = &mut self.state;
        state.media = media;
        state.chain.configure(media, &mut self.lane_state);
        state
            .resampler
            .configure(state.engine_sample_rate, media.sample_rate());
        state.pending.clear();
    }

    /// Load every voice's buffer from the WAV cache. Main thread.
    fn prepare_resources_impl(
        &mut self,
        wav_data: &HashMap<String, Arc<WavData>>,
        _sample_rate: f32,
    ) {
        let media = self.params.quality.media();
        let wav = wav_data
            .get(self.params.wav.path())
            .filter(|w| w.frame_count() > 0 && w.sample_rate() > 0.0);
        let (source, per_channel) = cached_load(wav, media);
        // A two-channel file gives each side its own channel; any other
        // gives each lane one channel on both sides.
        let channels = per_channel.len();
        let buffers = (0..self.lane_state.len())
            .map(|v| {
                let channel = if channels == 2 {
                    v % 2
                } else {
                    v / 2 % channels
                };
                let mut b = GrainBuffer::clone(&per_channel[channel]);
                b.set_source(source);
                b
            })
            .collect();
        self.params.loaded = Some(Loaded { source, buffers });
    }

    fn update(&mut self, _sample_rate: f32) {
        let params = &self.params;
        let state = &mut self.state;
        let voices = &mut self.lane_state;

        let index = state.resampler.input_index();
        for (i, v) in voices.iter_mut().enumerate() {
            let trigger = params.trigger.as_ref().map(|t| t.get_value(i));
            let onset = v.voice.sense(0.0, false, index, trigger, None);
            if let Some(age) = onset {
                let p = params;
                let grain = GrainTrigger {
                    voice: i,
                    fractional_sample: 0.0,
                    params: GrainParams {
                        // Grain positions count back from the end of the file.
                        time: 1.0 - unit(p.time.value_or(i, 0.0)),
                        spread: unit(p.spread.value_or(i, 0.0)),
                        ..grain_params(&None, &p.size, &p.shape, &p.freq, &p.root_freq, i)
                    },
                };
                state.pending.push(state.sample, age, grain);
            }
        }

        let GranularSamplerState {
            chain,
            resampler,
            rng,
            sample,
            pending,
            ..
        } = state;
        let now = *sample;
        *sample += 1;
        let ratio = resampler.ratio();
        let mut controls = |_| CONTROLS;

        let tap = resampler.tick(|inner, out_index| {
            if !pending.is_empty() {
                // No input passes through the resampler, so grains start
                // without waiting for one.
                pending.take_due(now, inner.ago, 0.0, ratio, |t| chain.offer(voices, t));
            }
            chain.process(voices, &mut controls, rng, out_index);
        });

        for (i, v) in voices.iter().enumerate() {
            self.outputs
                .sample
                .set(i, tap.read(&v.output) * FULL_SCALE_VOLTS);
        }
    }
}

impl PatchUpdateHandler for GranularSampler {
    fn on_patch_update(&mut self) {
        let media = self.params.quality.media();
        if media != self.state.media {
            self.configure(media);
        }
        // Hand each voice its loaded buffer unless it already holds the same
        // contents. The buffer it gives up drops with the params on the main
        // thread.
        if let Some(loaded) = &mut self.params.loaded {
            let source = loaded.source;
            for (v, b) in self.lane_state.iter_mut().zip(loaded.buffers.iter_mut()) {
                let buffer = &mut v.voice.granular.buffer;
                if buffer.source() != source && b.source() == source {
                    std::mem::swap(buffer, b);
                    v.voice.granular.player.reset();
                }
            }
        }
    }
}

message_handlers!(impl GranularSampler {});

#[cfg(test)]
#[path = "tests_sampler.rs"]
mod tests;
