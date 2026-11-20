//! Following the system default output device, and reopening an output
//! whose stream stopped once its device is back.

use std::time::Instant;

use napi_derive::napi;

use crate::audio::{self, AudioDeviceInfo, BufferSizeRange, preferred_default_sample_rate};
use crate::{Synthesizer, host_for_id, system_default_output_id};

/// What happened to the output device.
#[napi(string_enum)]
#[derive(Debug, PartialEq, Eq)]
pub enum OutputEvent {
    /// The output moved to a new system default device.
    Moved,
    /// The output's stream stopped and its device is not back yet. The
    /// engine waits, frozen, for it.
    Disconnected,
    /// The output's device is back and its streams reopened.
    Reconnected,
}

/// What changed when the output followed a new system default device or
/// lost and regained its own.
#[napi(object)]
#[derive(Debug)]
pub struct DeviceFollowReport {
    pub event: OutputEvent,
    /// The output device, when Shoreline lists it.
    pub output_device_name: Option<String>,
    /// The preferred input could not open at the sample rate the new output
    /// runs at, which leaves the input off.
    pub unopened_input_device_name: Option<String>,
    pub previous_sample_rate: u32,
    /// Differs from `previous_sample_rate` only when the new output cannot
    /// run at that rate.
    pub sample_rate: u32,
    /// The patch kept playing through the move, with its state.
    pub engine_kept: bool,
    /// The file of the recording the move ended.
    pub recording_path: Option<String>,
    /// Why the streams could not be opened on the new device.
    pub error: Option<String>,
    /// The streams could not move and the device they play on is gone, so
    /// nothing can be heard until the output changes again.
    pub output_lost: bool,
}

impl Synthesizer {
    /// The id of `host_id`'s system default output, with the device cache
    /// refreshed first when it does not list that device yet.
    pub(crate) fn default_output_id(&mut self, host_id: &str) -> Option<String> {
        let id = system_default_output_id(&host_for_id(host_id)?)?;
        if self.device_cache.find_output_device(&id).is_none() {
            self.refresh_device_cache(None);
        }
        Some(id)
    }
}

/// The settings the streams open with on a new output device.
#[derive(Debug, PartialEq)]
struct Settings {
    sample_rate: u32,
    /// The preferred input opens at `sample_rate`.
    input_opens: bool,
    buffer_size: Option<u32>,
}

/// The current sample rate where the new output allows it, so the engine
/// keeps playing; otherwise the preferred rate, then the output's own. The
/// preferred buffer size and input where the devices allow them.
fn choose_settings(
    output: &AudioDeviceInfo,
    input: Option<&AudioDeviceInfo>,
    preferred_sample_rate: u32,
    current_sample_rate: u32,
    preferred_buffer_size: Option<u32>,
) -> Settings {
    let supported = &output.supported_sample_rates;
    let sample_rate = if supported.contains(&current_sample_rate) {
        current_sample_rate
    } else if supported.contains(&preferred_sample_rate) {
        preferred_sample_rate
    } else {
        preferred_default_sample_rate(output.sample_rate, supported)
    };
    let input = input.filter(|input| input.supported_sample_rates.contains(&sample_rate));
    let fits = |range: &Option<BufferSizeRange>, size: &u32| {
        range
            .as_ref()
            .is_none_or(|range| (range.min..=range.max).contains(size))
    };
    let buffer_size = preferred_buffer_size.filter(|size| {
        fits(&output.buffer_size_range, size)
            && input.is_none_or(|input| fits(&input.buffer_size_range, size))
    });
    Settings {
        sample_rate,
        input_opens: input.is_some(),
        buffer_size,
    }
}

/// Check the output once: move to a new system default output, or reopen a
/// stream that stopped once its device is back. `None` when nothing changed.
pub(crate) fn follow(synth: &mut Synthesizer) -> Option<DeviceFollowReport> {
    follow_system_default(synth).or_else(|| reopen_stopped_stream(synth))
}

fn follow_system_default(synth: &mut Synthesizer) -> Option<DeviceFollowReport> {
    if !synth.follows_system_default_output
        || !audio::host_default_output_can_change(&synth.host_id)
    {
        return None;
    }
    let default_output = system_default_output_id(&host_for_id(&synth.host_id)?)?;
    if synth.seen_default_output.as_ref() == Some(&default_output) {
        return None;
    }
    synth.seen_default_output = Some(default_output.clone());
    synth.refresh_device_cache(None);
    let mut report = open_on(synth, OutputEvent::Moved, default_output);
    if report.error.is_some() {
        report.output_lost = output_lost(synth);
    }
    Some(report)
}

/// Reopen the output once its stream has stopped (the device unplugged or
/// reset) and the device is listed again. The first time a pinned device is
/// found gone reports `Disconnected`; a device followed as the system default
/// is replaced by the next default instead.
fn reopen_stopped_stream(synth: &mut Synthesizer) -> Option<DeviceFollowReport> {
    let now = Instant::now();
    if !synth.stream_watchdog.is_dead(&synth.stream_health, now)
        || !synth.stream_watchdog.take_retry(now)
    {
        return None;
    }
    let output_device_id = synth.output_device_id.clone()?;
    let name = synth
        .device_cache
        .find_output_device(&output_device_id)
        .map(|device| device.name.clone());
    synth.refresh_device_cache(None);
    let listed = synth
        .device_cache
        .find_output_device(&output_device_id)
        .is_some();
    if listed {
        let report = open_on(synth, OutputEvent::Reconnected, output_device_id);
        // A device that is listed but will not open yet is retried quietly.
        return report.error.is_none().then_some(report);
    }
    if synth.output_disconnected || synth.follows_system_default_output {
        return None;
    }
    synth.output_disconnected = true;
    let mut report = empty_report(synth, OutputEvent::Disconnected);
    report.output_device_name = name;
    Some(report)
}

fn empty_report(synth: &Synthesizer, event: OutputEvent) -> DeviceFollowReport {
    let sample_rate = synth.sample_rate as u32;
    DeviceFollowReport {
        event,
        output_device_name: None,
        unopened_input_device_name: None,
        previous_sample_rate: sample_rate,
        sample_rate,
        engine_kept: false,
        recording_path: None,
        error: None,
        output_lost: false,
    }
}

/// Open the streams on `output_device_id`, which the device cache was just
/// refreshed to list, with the settings it allows.
fn open_on(
    synth: &mut Synthesizer,
    event: OutputEvent,
    output_device_id: String,
) -> DeviceFollowReport {
    let mut report = empty_report(synth, event);
    let Some(output) = synth
        .device_cache
        .find_output_device(&output_device_id)
        .cloned()
    else {
        report.error = Some("it isn't a device Shoreline can open".to_string());
        return report;
    };
    report.output_device_name = Some(output.name.clone());

    let input = synth
        .preferred_input_device_id
        .as_ref()
        .and_then(|id| synth.device_cache.find_input_device(id).cloned());
    let settings = choose_settings(
        &output,
        input.as_ref(),
        synth.preferred_sample_rate,
        report.previous_sample_rate,
        synth.preferred_buffer_size,
    );
    let (input, unopened_input) = match input {
        Some(input) if settings.input_opens => (Some(input), None),
        unopened => (None, unopened),
    };

    match synth.open_streams(
        output_device_id,
        settings.sample_rate,
        settings.buffer_size,
        input.map(|input| input.id),
    ) {
        Ok(opened) => {
            report.sample_rate = settings.sample_rate;
            report.unopened_input_device_name = unopened_input.map(|input| input.name);
            report.engine_kept = opened.engine_kept;
            report.recording_path = opened.recording_path;
        }
        Err(error) => report.error = Some(error.reason.clone()),
    }
    report
}

/// The device the streams play on has left the (just refreshed) device list.
fn output_lost(synth: &Synthesizer) -> bool {
    synth
        .output_device_id
        .as_ref()
        .is_none_or(|id| synth.device_cache.find_output_device(id).is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(rates: &[u32], buffer: (u32, u32)) -> AudioDeviceInfo {
        AudioDeviceInfo {
            id: String::new(),
            host_id: String::new(),
            name: String::new(),
            input_channels: 2,
            output_channels: 2,
            is_default: true,
            sample_rate: rates[0],
            supported_sample_rates: rates.to_vec(),
            buffer_size_range: Some(BufferSizeRange {
                min: buffer.0,
                max: buffer.1,
            }),
        }
    }

    #[test]
    fn keeps_the_current_rate_and_preferred_settings_where_the_output_allows_them() {
        let output = device(&[44100, 48000, 96000], (15, 4096));
        let input = device(&[48000, 96000], (15, 4096));
        assert_eq!(
            choose_settings(&output, Some(&input), 48000, 96000, Some(64)),
            Settings {
                sample_rate: 96000,
                input_opens: true,
                buffer_size: Some(64),
            }
        );
    }

    #[test]
    fn falls_back_to_the_preferred_rate_then_the_outputs_own() {
        let output = device(&[44100, 48000], (15, 4096));
        assert_eq!(
            choose_settings(&output, None, 48000, 96000, None).sample_rate,
            48000
        );
        let only_44k = device(&[44100], (15, 4096));
        assert_eq!(
            choose_settings(&only_44k, None, 96000, 48000, None).sample_rate,
            44100
        );
    }

    #[test]
    fn an_input_that_cannot_run_at_the_rate_stays_off() {
        let output = device(&[48000], (15, 4096));
        let input = device(&[96000], (15, 4096));
        assert!(!choose_settings(&output, Some(&input), 96000, 96000, None).input_opens);
    }

    #[test]
    fn a_buffer_size_either_device_rejects_falls_back_to_the_hosts() {
        let output = device(&[48000], (128, 4096));
        assert_eq!(
            choose_settings(&output, None, 48000, 48000, Some(64)).buffer_size,
            None
        );
        let input = device(&[48000], (15, 32));
        let wide = device(&[48000], (15, 4096));
        assert_eq!(
            choose_settings(&wide, Some(&input), 48000, 48000, Some(64)).buffer_size,
            None
        );
    }
}
