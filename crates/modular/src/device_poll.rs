//! Polling the audio devices: following the system default output, moving to
//! a chosen output once it is connected, and reopening a stream that stopped.

use std::time::{Duration, Instant};

use napi_derive::napi;

use crate::Synthesizer;
use crate::audio;
use crate::output_choice::OutputChoice;
use crate::stream_resolve::{
    Fit, OpenRequest, OutputTarget, host_for_id, resolve, system_default_output_id,
};

/// How often the device list is read for a chosen output that is not
/// connected.
const MISSING_OUTPUT_CHECK: Duration = Duration::from_secs(3);

/// What happened to the audio devices.
#[napi(string_enum)]
#[derive(Debug, PartialEq, Eq)]
pub enum DeviceEvent {
    /// The output moved to a new system default device.
    Moved,
    /// The chosen output was not connected and now is, and the streams
    /// moved to it.
    Connected,
    /// The output's stream stopped and its device is not back yet. The
    /// engine waits frozen until it returns.
    Disconnected,
    /// The output's device is back and its streams reopened.
    Reconnected,
    /// The output's device changed its sample rate and the streams reopened
    /// at the new rate.
    RateChanged,
    /// The input's stream stopped and the streams reopened on the same
    /// output.
    InputReopened,
}

/// What a device poll changed.
#[napi(object)]
#[derive(Debug)]
pub struct DeviceReport {
    pub event: DeviceEvent,
    /// The output device, when Shoreline lists it.
    pub output_device_name: Option<String>,
    /// The input the streams opened with.
    pub input_device_name: Option<String>,
    /// The input whose stream stopped and whose device is gone, which leaves
    /// the input off.
    pub disconnected_input_device_name: Option<String>,
    /// The chosen input could not open at the sample rate the output runs
    /// at, which leaves the input off.
    pub unopened_input_device_name: Option<String>,
    pub previous_sample_rate: u32,
    /// Differs from `previous_sample_rate` when the output cannot run at that
    /// rate, or changed its own.
    pub sample_rate: u32,
    /// The buffer size in frames; `None` is the host's default.
    pub previous_buffer_size: Option<u32>,
    /// Differs from `previous_buffer_size` only when the output cannot use
    /// that buffer size.
    pub buffer_size: Option<u32>,
    /// A fresh engine replaced the one holding the patch, which stops
    /// playback.
    pub engine_reset: bool,
    /// The file of the recording the engine reset ended.
    pub recording_path: Option<String>,
    /// Why the streams could not open on the device.
    pub error: Option<String>,
    /// The streams could not open on the new default and the device they
    /// play on is gone, so nothing can be heard until the output changes
    /// again.
    pub output_lost: bool,
}

/// The checks in order, the first that changes something ending the poll,
/// then the module updates waiting on a frozen engine go out.
pub(crate) fn poll(synth: &mut Synthesizer) -> Option<DeviceReport> {
    synth.streams.adopt_returning_engine(Duration::ZERO);
    let report = move_to_missing_output(synth)
        .or_else(|| follow_system_default(synth))
        .or_else(|| reopen_stopped_stream(synth))
        .or_else(|| reopen_stopped_input(synth));
    synth.state.flush_commands();
    report
}

/// Move the streams to the chosen output once it is connected. A device that
/// is listed but will not open is reported once and retried.
fn move_to_missing_output(synth: &mut Synthesizer) -> Option<DeviceReport> {
    let now = Instant::now();
    let OutputChoice::Missing(missing) = &mut synth.output_choice else {
        return None;
    };
    if now < missing.next_check {
        return None;
    }
    missing.next_check = now + MISSING_OUTPUT_CHECK;
    let id = missing.id.clone();
    synth.refresh_device_cache(None);
    synth.device_cache.find_output_device(&id)?;
    let report = open_on(synth, DeviceEvent::Connected, id, true);
    if report.error.is_some() {
        let OutputChoice::Missing(missing) = &mut synth.output_choice else {
            return None;
        };
        return (!std::mem::replace(&mut missing.error_reported, true)).then_some(report);
    }
    synth.output_choice = OutputChoice::Device;
    Some(report)
}

/// Move to the system default output when it changed. A move that fails
/// while the current device is gone is reported once per stream.
fn follow_system_default(synth: &mut Synthesizer) -> Option<DeviceReport> {
    if !audio::host_default_output_can_change(&synth.streams.host_id) {
        return None;
    }
    let seen_default = synth.output_choice.seen_default()?;
    let default_output = system_default_output_id(&host_for_id(&synth.streams.host_id)?)?;
    if seen_default.as_ref() == Some(&default_output) {
        return None;
    }
    *seen_default = Some(default_output.clone());
    synth.refresh_device_cache(None);
    let mut report = open_on(synth, DeviceEvent::Moved, default_output, true);
    if report.error.is_some() {
        report.output_lost = !output_listed(synth);
        if report.output_lost && !synth.streams.watchdog.take_first_loss() {
            return None;
        }
    }
    Some(report)
}

/// Reopen the output once its stream has stopped. A device that changed its
/// sample rate reopens at the new rate. A device that left reopens once it is
/// listed again; until then System Default moves to the current default, and
/// otherwise `Disconnected` is reported once. A listed device that will not
/// open is reported once and retried.
fn reopen_stopped_stream(synth: &mut Synthesizer) -> Option<DeviceReport> {
    let now = Instant::now();
    let streams = &mut synth.streams;
    if !streams.watchdog.is_dead(&streams.output_health, now) || !streams.watchdog.take_retry(now) {
        return None;
    }
    let output_device_id = streams.output_device_id.clone();
    let name = synth.device_cache.output_name(&output_device_id);
    synth.refresh_device_cache(None);
    if output_listed(synth) {
        let report = if synth.streams.output_health.was_invalidated() {
            open_on(synth, DeviceEvent::RateChanged, output_device_id, false)
        } else {
            open_on(synth, DeviceEvent::Reconnected, output_device_id, true)
        };
        return (report.error.is_none() || synth.streams.watchdog.take_first_loss())
            .then_some(report);
    }
    if let Some(seen_default) = synth.output_choice.seen_default() {
        *seen_default = None;
        if let Some(report) = follow_system_default(synth) {
            return Some(report);
        }
    }
    if !synth.streams.watchdog.take_first_loss() {
        return None;
    }
    let mut report = empty_report(synth, DeviceEvent::Disconnected);
    report.output_device_name = name;
    Some(report)
}

/// Reopen the streams on the same output once the input's stream has
/// stopped, with the chosen input where it is listed and runs at the
/// engine's rate. A reopen that fails is reported once and retried.
fn reopen_stopped_input(synth: &mut Synthesizer) -> Option<DeviceReport> {
    let streams = &mut synth.streams;
    let input_device_id = streams.input_device_id.clone()?;
    if !streams.input_health.has_ended() || !streams.watchdog.take_retry(Instant::now()) {
        return None;
    }
    let output_device_id = streams.output_device_id.clone();
    let input_name = synth.device_cache.input_name(&input_device_id);
    synth.refresh_device_cache(None);
    let input_listed = synth
        .device_cache
        .find_input_device(&input_device_id)
        .is_some();
    let mut report = open_on(synth, DeviceEvent::InputReopened, output_device_id, true);
    if report.error.is_some() {
        return synth.streams.watchdog.take_first_loss().then_some(report);
    }
    if !input_listed {
        report.disconnected_input_device_name = input_name;
    }
    Some(report)
}

fn empty_report(synth: &Synthesizer, event: DeviceEvent) -> DeviceReport {
    let sample_rate = synth.streams.sample_rate;
    DeviceReport {
        event,
        output_device_name: None,
        input_device_name: None,
        disconnected_input_device_name: None,
        unopened_input_device_name: None,
        previous_sample_rate: sample_rate,
        sample_rate,
        previous_buffer_size: synth.streams.buffer_size,
        buffer_size: synth.streams.buffer_size,
        engine_reset: false,
        recording_path: None,
        error: None,
        output_lost: false,
    }
}

/// Open the streams on `output_device_id`, which the device cache was just
/// refreshed to list, with the settings it allows. `keep_rate` holds the
/// engine's sample rate where the device runs at it; otherwise the device's
/// own rate comes first. The engine's buffer size is held where the device
/// allows it, and the chosen settings come next.
fn open_on(
    synth: &mut Synthesizer,
    event: DeviceEvent,
    output_device_id: String,
    keep_rate: bool,
) -> DeviceReport {
    let mut report = empty_report(synth, event);
    let Some(output) = synth.device_cache.find_output_device(&output_device_id) else {
        report.error = Some("it isn't a device Shoreline can open".to_string());
        return report;
    };
    report.output_device_name = Some(output.name.clone());
    let sample_rate = if keep_rate {
        report.previous_sample_rate
    } else {
        output.sample_rate
    };

    let request = OpenRequest {
        output: OutputTarget::Listed(output_device_id),
        buffer_size: report.previous_buffer_size,
        input_device_id: synth.chosen_input_device_id.clone(),
        fit: Fit::Closest {
            sample_rate,
            fallback: (synth.chosen_sample_rate, synth.chosen_buffer_size),
        },
    };
    let mut unopened_input = None;
    let opened = resolve(&synth.device_cache, request).and_then(|mut plan| {
        unopened_input = plan.unopened_input.take();
        synth.open_streams(plan)
    });
    match opened {
        Ok(change) => {
            let cache = &synth.device_cache;
            report.input_device_name = synth
                .streams
                .input_device_id
                .as_deref()
                .and_then(|id| cache.input_name(id));
            report.unopened_input_device_name = unopened_input
                .as_deref()
                .and_then(|id| cache.input_name(id));
            report.sample_rate = synth.streams.sample_rate;
            report.buffer_size = synth.streams.buffer_size;
            report.engine_reset = change.engine_reset;
            report.recording_path = change.recording_path;
        }
        Err(error) => report.error = Some(error.reason.clone()),
    }
    report
}

/// The device the streams play on is in the device list.
fn output_listed(synth: &Synthesizer) -> bool {
    synth
        .device_cache
        .find_output_device(&synth.streams.output_device_id)
        .is_some()
}
