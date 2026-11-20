//! Following the system default output device.

use napi_derive::napi;

use crate::audio::{self, preferred_default_sample_rate};
use crate::{Synthesizer, host_for_id, system_default_output_id};

/// What changed when the output followed a new system default device.
#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct DeviceFollowReport {
    pub output_device_name: String,
    /// The input could not open at the sample rate the new output runs at,
    /// which leaves the input off.
    pub unopened_input_device_name: Option<String>,
    pub previous_sample_rate: u32,
    pub sample_rate: u32,
    /// Why the streams could not be opened on the new device, which leaves
    /// the previous streams running.
    pub error: Option<String>,
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
    let output = synth
        .device_cache
        .find_output_device(&default_output)?
        .clone();

    let previous_sample_rate = synth.sample_rate as u32;
    let sample_rate = if output
        .supported_sample_rates
        .contains(&previous_sample_rate)
    {
        previous_sample_rate
    } else {
        preferred_default_sample_rate(output.sample_rate, &output.supported_sample_rates)
    };

    let wanted_input = synth
        .input_device_id
        .as_ref()
        .and_then(|id| synth.device_cache.find_input_device(id).cloned());
    let (input, unopened_input) = match wanted_input {
        Some(input) if input.supported_sample_rates.contains(&sample_rate) => (Some(input), None),
        Some(input) => (None, Some(input)),
        None => (None, None),
    };

    let fits = |range: &Option<audio::BufferSizeRange>, size: &u32| {
        range
            .as_ref()
            .is_none_or(|range| (range.min..=range.max).contains(size))
    };
    let buffer_size = synth.buffer_size.filter(|size| {
        fits(&output.buffer_size_range, size)
            && input
                .as_ref()
                .is_none_or(|input| fits(&input.buffer_size_range, size))
    });

    let mut report = DeviceFollowReport {
        output_device_name: output.name.clone(),
        unopened_input_device_name: unopened_input.map(|input| input.name),
        previous_sample_rate,
        sample_rate,
        error: None,
    };
    if let Err(error) = synth.open_streams(
        default_output,
        sample_rate,
        buffer_size,
        input.map(|input| input.id),
    ) {
        report.error = Some(error.reason.clone());
        report.sample_rate = previous_sample_rate;
    }
    Some(report)
}
