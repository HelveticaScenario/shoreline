//! Resolving a stream open: which devices the streams open on and the
//! settings they run at, from what was asked and what the devices allow.

use cpal::traits::{DeviceTrait, HostTrait};
use napi::Result;

use crate::AudioConfigOptions;
use crate::audio::{
    AudioDeviceCache, AudioDeviceInfo, BufferSizeRange, find_input_device_in_host,
    find_output_device_in_host, get_host_by_preference, preferred_default_sample_rate,
};

/// The output device a stream open asks for.
pub(crate) enum OutputTarget {
    /// A device the device cache lists.
    Listed(String),
    /// The host's own default output, which the device cache does not list.
    /// A stream on it reroutes itself when the default changes, so only a new
    /// synthesizer with no listed default opens it.
    Unlisted(cpal::Host),
}

/// How the settings asked for give way to what the output allows.
pub(crate) enum Fit {
    /// Exactly the settings asked for, or an error.
    Exact,
    /// The settings asked for where the output allows them, then
    /// `fallback`'s, then the output's own. The input opens only where it
    /// runs at the chosen sample rate.
    Closest { fallback: (u32, Option<u32>) },
    /// The sample rate asked for where the output runs at it, otherwise the
    /// output's preferred default; the buffer size as asked; an input that
    /// will not open is left off. A new synthesizer opens this way.
    Launch,
}

/// A stream open: the devices and settings asked for.
pub(crate) struct OpenRequest {
    pub(crate) output: OutputTarget,
    /// `None` asks for the output's preferred default rate.
    pub(crate) sample_rate: Option<u32>,
    pub(crate) buffer_size: Option<u32>,
    /// `None`, `"none"` or an empty id opens no input.
    pub(crate) input_device_id: Option<String>,
    pub(crate) fit: Fit,
}

/// The devices and settings a stream open resolved to.
pub(crate) struct StreamPlan {
    pub(crate) host_id: String,
    pub(crate) output_device_id: String,
    pub(crate) output_device: cpal::Device,
    pub(crate) output_config: cpal::SupportedStreamConfig,
    pub(crate) sample_rate: u32,
    pub(crate) buffer_size: Option<u32>,
    pub(crate) input: Option<(cpal::Device, cpal::SupportedStreamConfig, String)>,
    /// The input asked for, listed but left off because it cannot run at
    /// `sample_rate`.
    pub(crate) unopened_input: Option<String>,
    /// The sample rate asked for, which the output cannot run at.
    pub(crate) unsupported_sample_rate: Option<u32>,
}

/// The settings the streams open with on an output device.
#[derive(Debug, PartialEq)]
struct Settings {
    sample_rate: u32,
    /// The input opens at `sample_rate`.
    input_opens: bool,
    buffer_size: Option<u32>,
}

/// The current sample rate and buffer size where the output allows them;
/// otherwise the preferred ones, then the output's own. The input where it
/// runs at the chosen rate.
fn choose_settings(
    output: &AudioDeviceInfo,
    input: Option<&AudioDeviceInfo>,
    preferred: (u32, Option<u32>),
    current: (u32, Option<u32>),
) -> Settings {
    let (preferred_sample_rate, preferred_buffer_size) = preferred;
    let (current_sample_rate, current_buffer_size) = current;
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
    let allowed = |size: &Option<u32>| {
        size.is_none_or(|size| {
            fits(&output.buffer_size_range, &size)
                && input.is_none_or(|input| fits(&input.buffer_size_range, &size))
        })
    };
    let buffer_size = [current_buffer_size, preferred_buffer_size]
        .into_iter()
        .find(allowed)
        .flatten();
    Settings {
        sample_rate,
        input_opens: input.is_some(),
        buffer_size,
    }
}

/// Resolve `request` against the device cache to the devices and settings
/// the streams open with.
pub(crate) fn resolve(cache: &AudioDeviceCache, request: OpenRequest) -> Result<StreamPlan> {
    let (host, host_id, output_device_id, output_device, output) = match request.output {
        OutputTarget::Listed(id) => {
            let output = cache.find_output_device(&id).ok_or_else(|| {
                napi::Error::from_reason(format!(
                    "Output device '{}' not found in cache. Try refreshing the device cache.",
                    id
                ))
            })?;
            let host_id = output.host_id.clone();
            let host = host_for_id(&host_id).ok_or_else(|| {
                napi::Error::from_reason(format!("Failed to get host '{}'", host_id))
            })?;
            let device = find_output_device_in_host(&host, &id).ok_or_else(|| {
                napi::Error::from_reason(format!(
                    "Output device '{}' not found in host '{}'",
                    id, host_id
                ))
            })?;
            (host, host_id, id, device, Some(output))
        }
        OutputTarget::Unlisted(host) => {
            let device = host
                .default_output_device()
                .ok_or_else(|| napi::Error::from_reason("No output device is available"))?;
            let id = device.id().map(|id| id.to_string()).map_err(|err| {
                napi::Error::from_reason(format!("The output device has no id: {err}"))
            })?;
            let host_id = format!("{:?}", host.id());
            (host, host_id, id, device, None)
        }
    };
    let output_config = output_device
        .default_output_config()
        .map_err(|e| napi::Error::from_reason(format!("Failed to get output config: {}", e)))?;

    let asked_input = chosen_input(request.input_device_id);
    let mut unopened_input = None;
    let mut unsupported_sample_rate = None;
    let (sample_rate, buffer_size, input_device_id) = match (&request.fit, output) {
        (Fit::Closest { fallback }, Some(output)) => {
            let input_info = asked_input
                .as_ref()
                .and_then(|id| cache.find_input_device(id));
            let settings = choose_settings(
                output,
                input_info,
                *fallback,
                (
                    request.sample_rate.unwrap_or(output.sample_rate),
                    request.buffer_size,
                ),
            );
            let input = match asked_input.filter(|_| input_info.is_some()) {
                Some(id) if settings.input_opens => Some(id),
                unopened => {
                    unopened_input = unopened;
                    None
                }
            };
            (settings.sample_rate, settings.buffer_size, input)
        }
        (Fit::Launch, Some(output)) => {
            let supported = &output.supported_sample_rates;
            let sample_rate = request
                .sample_rate
                .filter(|rate| supported.contains(rate))
                .unwrap_or_else(|| {
                    unsupported_sample_rate = request.sample_rate;
                    preferred_default_sample_rate(output_config.sample_rate(), supported)
                });
            (sample_rate, request.buffer_size, asked_input)
        }
        (Fit::Exact, Some(output)) => {
            let sample_rate = request.sample_rate.unwrap_or_else(|| {
                preferred_default_sample_rate(output.sample_rate, &output.supported_sample_rates)
            });
            if !output.supported_sample_rates.contains(&sample_rate) {
                return Err(napi::Error::from_reason(format!(
                    "Sample rate {}Hz not supported by output device",
                    sample_rate
                )));
            }
            (sample_rate, request.buffer_size, asked_input)
        }
        (_, None) => (
            output_config.sample_rate(),
            request.buffer_size,
            asked_input,
        ),
    };

    let input = match input_device_id {
        None => None,
        Some(id) if matches!(request.fit, Fit::Launch) => find_input_device_in_host(&host, &id)
            .and_then(|device| {
                let config = device
                    .default_input_config()
                    .inspect_err(|e| eprintln!("Failed to get input config: {}", e))
                    .ok()?;
                Some((device, config, id))
            }),
        Some(id) => Some(listed_input(cache, &host, &host_id, id, sample_rate)?),
    };

    Ok(StreamPlan {
        host_id,
        output_device_id,
        output_device,
        output_config,
        sample_rate,
        buffer_size,
        input,
        unopened_input,
        unsupported_sample_rate,
    })
}

/// The listed input `id` on `host`, which must run at `sample_rate`.
fn listed_input(
    cache: &AudioDeviceCache,
    host: &cpal::Host,
    host_id: &str,
    id: String,
    sample_rate: u32,
) -> Result<(cpal::Device, cpal::SupportedStreamConfig, String)> {
    let info = cache.find_input_device(&id).ok_or_else(|| {
        napi::Error::from_reason(format!("Input device '{}' not found in cache", id))
    })?;
    if info.host_id != host_id {
        return Err(napi::Error::from_reason(format!(
            "Input device '{}' is on host '{}' but output device is on host '{}'. Both must be on the same host.",
            id, info.host_id, host_id
        )));
    }
    if !info.supported_sample_rates.contains(&sample_rate) {
        return Err(napi::Error::from_reason(format!(
            "Sample rate {}Hz not supported by input device",
            sample_rate
        )));
    }
    let device = find_input_device_in_host(host, &id).ok_or_else(|| {
        napi::Error::from_reason(format!("Input device '{}' not found in host", id))
    })?;
    let config = device
        .default_input_config()
        .map_err(|e| napi::Error::from_reason(format!("Failed to get input config: {}", e)))?;
    Ok((device, config, id))
}

/// What a new synthesizer opens for `config`: the host and output asked for,
/// falling back to the preferred host and the system default output where
/// they are not available, and the input asked for, falling back to the
/// host's default input. Appends why to `warning`.
pub(crate) fn launch_request(
    cache: &AudioDeviceCache,
    config: &AudioConfigOptions,
    warning: &mut Option<String>,
) -> OpenRequest {
    let host = match &config.host_id {
        None => get_host_by_preference(),
        Some(requested) => match parse_host_id(requested) {
            Some(id) => cpal::host_from_id(id).unwrap_or_else(|_| {
                *warning = Some(format!(
                    "Requested host '{}' not available, using default. ",
                    requested
                ));
                get_host_by_preference()
            }),
            None => {
                *warning = Some(format!("Unknown host '{}', using default. ", requested));
                get_host_by_preference()
            }
        },
    };
    let host_id = format!("{:?}", host.id());
    let on_host = |device: Option<&AudioDeviceInfo>| device.is_some_and(|d| d.host_id == host_id);
    let mut append = |text: String| {
        *warning = Some(format!("{}{}", warning.take().unwrap_or_default(), text));
    };

    let listed_output = match &config.output_device_id {
        Some(id) if on_host(cache.find_output_device(id)) => Some(id.clone()),
        requested => {
            if let Some(id) = requested {
                append(format!(
                    "Output device '{}' is not connected, so the sound plays on the system default until it is. ",
                    id
                ));
            }
            system_default_output_id(&host).filter(|id| on_host(cache.find_output_device(id)))
        }
    };

    let input_device_id = chosen_input(config.input_device_id.clone()).and_then(|id| {
        if on_host(cache.find_input_device(&id)) {
            return Some(id);
        }
        append(format!("Input device '{}' not found, using default. ", id));
        host.default_input_device()
            .and_then(|device| device.id().ok())
            .map(|id| id.to_string())
    });

    OpenRequest {
        output: match listed_output {
            Some(id) => OutputTarget::Listed(id),
            None => OutputTarget::Unlisted(host),
        },
        sample_rate: config.sample_rate,
        buffer_size: config.buffer_size,
        input_device_id,
        fit: Fit::Launch,
    }
}

/// The input device an id asks for: `None` for no input, which the renderer
/// sends as `"none"` or an empty id.
pub(crate) fn chosen_input(input_device_id: Option<String>) -> Option<String> {
    input_device_id.filter(|id| !id.is_empty() && id != "none")
}

pub(crate) fn host_for_id(host_id: &str) -> Option<cpal::Host> {
    parse_host_id(host_id).and_then(|id| cpal::host_from_id(id).ok())
}

/// The id of the host's system default output. A stream built from
/// `host.default_output_device()` reroutes itself when the default changes,
/// behind the engine's back, so streams open on the device this id lists,
/// which stays put, and polling moves them to a new default.
pub(crate) fn system_default_output_id(host: &cpal::Host) -> Option<String> {
    let device = host.default_output_device()?;
    device.id().ok().map(|id| id.to_string())
}

/// Parse a host ID string to cpal::HostId
fn parse_host_id(id: &str) -> Option<cpal::HostId> {
    match id {
        #[cfg(target_os = "macos")]
        "CoreAudio" => Some(cpal::HostId::CoreAudio),
        #[cfg(target_os = "windows")]
        "Wasapi" | "WASAPI" => Some(cpal::HostId::Wasapi),
        #[cfg(target_os = "linux")]
        "Alsa" | "ALSA" => Some(cpal::HostId::Alsa),
        #[cfg(target_os = "linux")]
        "Jack" | "JACK" => Some(cpal::HostId::Jack),
        _ => None,
    }
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
    fn keeps_the_current_rate_and_buffer_size_where_the_output_allows_them() {
        let output = device(&[44100, 48000, 96000], (15, 4096));
        let input = device(&[48000, 96000], (15, 4096));
        assert_eq!(
            choose_settings(&output, Some(&input), (48000, Some(64)), (96000, Some(256))),
            Settings {
                sample_rate: 96000,
                input_opens: true,
                buffer_size: Some(256),
            }
        );
    }

    #[test]
    fn falls_back_to_the_preferred_rate_then_the_outputs_own() {
        let output = device(&[44100, 48000], (15, 4096));
        assert_eq!(
            choose_settings(&output, None, (48000, None), (96000, None)).sample_rate,
            48000
        );
        let only_44k = device(&[44100], (15, 4096));
        assert_eq!(
            choose_settings(&only_44k, None, (96000, None), (48000, None)).sample_rate,
            44100
        );
    }

    #[test]
    fn an_input_that_cannot_run_at_the_rate_stays_off() {
        let output = device(&[48000], (15, 4096));
        let input = device(&[96000], (15, 4096));
        assert!(!choose_settings(&output, Some(&input), (96000, None), (96000, None)).input_opens);
    }

    #[test]
    fn a_buffer_size_the_output_rejects_falls_back_to_the_preferred_then_the_hosts() {
        let output = device(&[48000], (128, 4096));
        assert_eq!(
            choose_settings(&output, None, (48000, Some(256)), (48000, Some(64))).buffer_size,
            Some(256)
        );
        assert_eq!(
            choose_settings(&output, None, (48000, Some(64)), (48000, Some(32))).buffer_size,
            None
        );
        let input = device(&[48000], (15, 32));
        let wide = device(&[48000], (15, 4096));
        assert_eq!(
            choose_settings(&wide, Some(&input), (48000, Some(64)), (48000, Some(64))).buffer_size,
            None
        );
    }
}
