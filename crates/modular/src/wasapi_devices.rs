//! WASAPI device enumeration from endpoint property stores and mix formats.
//!
//! cpal's per-device queries trial up to 49 formats with `IsFormatSupported`,
//! each a call into the audio service. Here each endpoint costs one client
//! activation: names come from the property store, and channels and rate from
//! the shared-mode mix format, which is what cpal's default config reports and
//! the streams open with. Shared mode accepts only the mix format's rate.

use std::ffi::c_void;

use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName,
};
use windows::Win32::Foundation::{PROPERTYKEY, RPC_E_CHANGED_MODE};
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, IAudioClient, IMMDevice, IMMDeviceEnumerator, IMMEndpoint,
    MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eAll, eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, STGM_READ,
};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::core::{GUID, Interface};

use crate::audio::AudioDeviceInfo;

/// Keeps COM initialized for the enumeration. COM may already be initialized
/// on this thread in another apartment mode, which serves just as well.
struct ComScope {
    initialized: bool,
}

impl ComScope {
    fn enter() -> Option<Self> {
        // SAFETY: balanced by `CoUninitialize` in `drop` when it succeeds.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        (result.is_ok() || result == RPC_E_CHANGED_MODE).then_some(Self {
            initialized: result.is_ok(),
        })
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: pairs the successful `CoInitializeEx` in `enter`.
            unsafe { CoUninitialize() };
        }
    }
}

/// A property value, cleared when dropped.
struct PropValue(PROPVARIANT);

impl Drop for PropValue {
    fn drop(&mut self) {
        // SAFETY: the value came from `IPropertyStore::GetValue` and is
        // cleared exactly once.
        unsafe {
            let _ = PropVariantClear(&mut self.0);
        }
    }
}

fn get_value(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<PropValue> {
    // SAFETY: `key` points to a valid PROPERTYKEY for the call.
    unsafe { store.GetValue(key) }.ok().map(PropValue)
}

fn read_string(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    let value = get_value(store, key)?;
    // SAFETY: the union is read as the member its `vt` tag names.
    unsafe {
        let inner = &value.0.Anonymous.Anonymous;
        if inner.vt != VT_LPWSTR {
            return None;
        }
        inner.Anonymous.pwszVal.to_string().ok()
    }
}

const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const KSDATAFORMAT_SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID =
    GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

/// Whether cpal maps this mix format to a sample format; its default config
/// fails, and the device cannot open, when it does not.
///
/// # Safety
/// `format` must point to a valid `WAVEFORMATEX`, followed by the rest of a
/// `WAVEFORMATEXTENSIBLE` when its tag says so.
unsafe fn cpal_can_open(format: *const WAVEFORMATEX) -> bool {
    let (tag, bits) = unsafe { ((*format).wFormatTag, (*format).wBitsPerSample) };
    match tag {
        WAVE_FORMAT_PCM => bits == 8 || bits == 16,
        WAVE_FORMAT_IEEE_FLOAT => bits == 32,
        WAVE_FORMAT_EXTENSIBLE => {
            let sub = unsafe { std::ptr::read_unaligned(format as *const WAVEFORMATEXTENSIBLE) }
                .SubFormat;
            (sub == KSDATAFORMAT_SUBTYPE_PCM && matches!(bits, 8 | 16 | 24 | 32 | 64))
                || (sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT && bits == 32)
        }
        _ => false,
    }
}

/// `(channels, sample_rate)` of the shared-mode mix format, or `None` when
/// the endpoint cannot be activated or cpal could not open its format.
fn mix_format(device: &IMMDevice) -> Option<(u16, u32)> {
    // SAFETY: the mix format is allocated by the client and freed here after
    // its fields are copied out.
    unsafe {
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;
        let format = client.GetMixFormat().ok()?;
        if format.is_null() {
            return None;
        }
        let result = cpal_can_open(format).then(|| ((*format).nChannels, (*format).nSamplesPerSec));
        CoTaskMemFree(Some(format as *const c_void));
        result
    }
}

fn endpoint_id(device: &IMMDevice) -> Option<String> {
    // SAFETY: `GetId` returns a CoTaskMem string, freed after copying.
    unsafe {
        let id = device.GetId().ok()?;
        let result = id.to_string().ok();
        CoTaskMemFree(Some(id.0 as *const c_void));
        result
    }
}

fn default_endpoint_id(enumerator: &IMMDeviceEnumerator, flow: EDataFlow) -> Option<String> {
    // SAFETY: plain COM call on a live enumerator.
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }.ok()?;
    endpoint_id(&device)
}

/// Enumerate active WASAPI endpoints, returning `(outputs, inputs)`. Ids match
/// cpal's `DeviceId` strings, so they resolve through cpal when a stream is
/// opened.
pub fn enumerate_devices(host_id: &str) -> (Vec<AudioDeviceInfo>, Vec<AudioDeviceInfo>) {
    let mut outputs = Vec::new();
    let mut inputs = Vec::new();

    let Some(_com) = ComScope::enter() else {
        return (outputs, inputs);
    };
    // SAFETY: COM is initialized for the scope of this function.
    let enumerator: IMMDeviceEnumerator =
        match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
            Ok(enumerator) => enumerator,
            Err(_) => return (outputs, inputs),
        };
    // SAFETY: plain COM calls on a live enumerator.
    let Ok(collection) = (unsafe { enumerator.EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE) })
    else {
        return (outputs, inputs);
    };
    let count = unsafe { collection.GetCount() }.unwrap_or(0);
    let default_output = default_endpoint_id(&enumerator, eRender);
    let default_input = default_endpoint_id(&enumerator, eCapture);

    for index in 0..count {
        // SAFETY: `index` is below the collection's count.
        let Ok(device) = (unsafe { collection.Item(index) }) else {
            continue;
        };
        let Some(endpoint_id) = endpoint_id(&device) else {
            continue;
        };
        // SAFETY: plain COM calls on a live device.
        let Ok(flow) = device
            .cast::<IMMEndpoint>()
            .and_then(|endpoint| unsafe { endpoint.GetDataFlow() })
        else {
            continue;
        };
        let Ok(store) = (unsafe { device.OpenPropertyStore(STGM_READ) }) else {
            continue;
        };
        let Some(name) = read_string(&store, &PKEY_Device_FriendlyName)
            .or_else(|| read_string(&store, &PKEY_Device_DeviceDesc))
        else {
            continue;
        };
        let Some((channels, sample_rate)) = mix_format(&device) else {
            continue;
        };

        let is_output = flow == eRender;
        let info = AudioDeviceInfo {
            is_default: if is_output {
                default_output.as_deref() == Some(endpoint_id.as_str())
            } else {
                default_input.as_deref() == Some(endpoint_id.as_str())
            },
            id: cpal::DeviceId(cpal::HostId::Wasapi, endpoint_id).to_string(),
            host_id: host_id.to_string(),
            name,
            input_channels: if is_output { 0 } else { channels },
            output_channels: if is_output { channels } else { 0 },
            sample_rate,
            supported_sample_rates: vec![sample_rate],
            buffer_size_range: None,
        };
        if is_output {
            outputs.push(info);
        } else if flow == eCapture {
            inputs.push(info);
        }
    }

    (outputs, inputs)
}
