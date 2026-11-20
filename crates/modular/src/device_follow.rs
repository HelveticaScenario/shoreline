//! Following the system default output device.

use napi_derive::napi;

use crate::audio::{self, AudioDeviceInfo, BufferSizeRange, preferred_default_sample_rate};
use crate::{Synthesizer, host_for_id, system_default_output_id};

/// What changed when the output followed a new system default device.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DeviceFollowReport {
    /// The new default output, when Shoreline lists it.
    pub output_device_name: Option<String>,
    /// The preferred input could not open at the sample rate the new output
    /// runs at, which leaves the input off.
    pub unopened_input_device_name: Option<String>,
    pub previous_sample_rate: u32,
    pub sample_rate: u32,
    /// The rate last asked for, which `sample_rate` differs from when the
    /// new output cannot run at it.
    pub preferred_sample_rate: u32,
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

/// The preferred sample rate, buffer size and input where the new output
/// allows them; otherwise the current rate, then the output's own.
fn choose_settings(
    output: &AudioDeviceInfo,
    input: Option<&AudioDeviceInfo>,
    preferred_sample_rate: u32,
    current_sample_rate: u32,
    preferred_buffer_size: Option<u32>,
) -> Settings {
    let supported = &output.supported_sample_rates;
    let sample_rate = if supported.contains(&preferred_sample_rate) {
        preferred_sample_rate
    } else if supported.contains(&current_sample_rate) {
        current_sample_rate
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

pub(crate) fn follow(synth: &mut Synthesizer) -> Option<DeviceFollowReport> {
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

    let previous_sample_rate = synth.sample_rate as u32;
    let mut report = DeviceFollowReport {
        output_device_name: None,
        unopened_input_device_name: None,
        previous_sample_rate,
        sample_rate: previous_sample_rate,
        preferred_sample_rate: synth.preferred_sample_rate,
        recording_path: None,
        error: None,
        output_lost: false,
    };
    let Some(output) = synth
        .device_cache
        .find_output_device(&default_output)
        .cloned()
    else {
        report.error = Some("it isn't a device Shoreline can open".to_string());
        report.output_lost = output_lost(synth);
        return Some(report);
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
        previous_sample_rate,
        synth.preferred_buffer_size,
    );
    let (input, unopened_input) = match input {
        Some(input) if settings.input_opens => (Some(input), None),
        unopened => (None, unopened),
    };

    match synth.open_streams(
        default_output,
        settings.sample_rate,
        settings.buffer_size,
        input.map(|input| input.id),
    ) {
        Ok(recording_path) => {
            report.sample_rate = settings.sample_rate;
            report.unopened_input_device_name = unopened_input.map(|input| input.name);
            report.recording_path = recording_path;
        }
        Err(error) => {
            report.error = Some(error.reason.clone());
            report.output_lost = output_lost(synth);
        }
    }
    Some(report)
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
    fn keeps_the_preferred_settings_where_the_output_allows_them() {
        let output = device(&[44100, 48000, 96000], (15, 4096));
        let input = device(&[48000, 96000], (15, 4096));
        assert_eq!(
            choose_settings(&output, Some(&input), 96000, 48000, Some(64)),
            Settings {
                sample_rate: 96000,
                input_opens: true,
                buffer_size: Some(64),
            }
        );
    }

    #[test]
    fn falls_back_to_the_current_rate_then_the_outputs_own() {
        let output = device(&[44100, 48000], (15, 4096));
        assert_eq!(
            choose_settings(&output, None, 96000, 48000, None).sample_rate,
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
