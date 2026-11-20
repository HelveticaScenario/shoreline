//! CoreAudio device enumeration from plain HAL property reads.
//!
//! cpal opens a HAL AudioUnit for each config query, tens of milliseconds per
//! device; every value here is a direct `AudioObjectGetPropertyData` read. The
//! values match cpal's: channels and rate come from
//! `kAudioDevicePropertyStreamFormat`, which the streams open with.

use std::mem;
use std::ptr::{NonNull, null};

use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectPropertyScope, AudioObjectPropertySelector,
    kAudioDevicePropertyAvailableNominalSampleRates, kAudioDevicePropertyBufferFrameSizeRange,
    kAudioDevicePropertyDeviceUID, kAudioDevicePropertyStreamFormat,
    kAudioHardwarePropertyDefaultInputDevice, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioHardwarePropertyDevices, kAudioObjectPropertyElementMain, kAudioObjectPropertyName,
    kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeInput,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
};
use objc2_core_audio_types::{
    AudioStreamBasicDescription, AudioValueRange, kAudioFormatFlagIsFloat,
    kAudioFormatFlagIsPacked, kAudioFormatFlagIsSignedInteger, kAudioFormatLinearPCM,
};
use objc2_core_foundation::{CFRetained, CFString};

use crate::audio::{AudioDeviceInfo, BufferSizeRange, COMMON_SAMPLE_RATES};

fn address(
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// Read a fixed-size property.
///
/// # Safety
/// `T` must be the property's data type.
unsafe fn read<T: Copy>(object: AudioObjectID, address: &AudioObjectPropertyAddress) -> Option<T> {
    // SAFETY: zeroed is a valid placeholder for the plain C data types read
    // here, and the HAL writes at most `size` bytes into it.
    let mut value: T = unsafe { mem::zeroed() };
    let mut size = mem::size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0 && size as usize == mem::size_of::<T>()).then_some(value)
}

/// Read a property holding an array of `T`.
///
/// # Safety
/// `T` must be the property's element type.
unsafe fn read_array<T: Copy>(
    object: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<Vec<T>> {
    let mut size = 0u32;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            object,
            NonNull::from(address),
            0,
            null(),
            NonNull::from(&mut size),
        )
    };
    if status != 0 {
        return None;
    }
    let mut values: Vec<T> = Vec::with_capacity(size as usize / mem::size_of::<T>());
    if values.capacity() == 0 {
        return Some(values);
    }
    // Offer the HAL only the bytes reserved: a reported size that is not a
    // whole number of elements would otherwise let it write past them.
    size = (values.capacity() * mem::size_of::<T>()) as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::new(values.as_mut_ptr())?.cast(),
        )
    };
    if status != 0 {
        return None;
    }
    // SAFETY: the HAL wrote `size` bytes, at most the reserved capacity.
    unsafe { values.set_len((size as usize / mem::size_of::<T>()).min(values.capacity())) };
    Some(values)
}

/// Read a CFString property. These follow the copy rule, so the returned
/// reference is owned and released when dropped.
fn read_string(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Option<String> {
    let address = address(selector, kAudioObjectPropertyScopeGlobal);
    // SAFETY: name and UID properties are `CFStringRef`.
    let string: *mut CFString = unsafe { read(object, &address)? };
    // SAFETY: the HAL hands over a +1 reference.
    let string = unsafe { CFRetained::from_raw(NonNull::new(string)?) };
    Some(string.to_string())
}

/// The stream format a direction opens with, as `(channels, sample_rate)`, or
/// `None` when the device has no usable stream in that scope. Accepts exactly
/// the formats cpal's default config accepts: packed 32-bit float or packed
/// signed 16-bit linear PCM.
fn default_format(object: AudioObjectID, scope: AudioObjectPropertyScope) -> Option<(u16, u32)> {
    let address = address(kAudioDevicePropertyStreamFormat, scope);
    // SAFETY: the stream format property is an `AudioStreamBasicDescription`.
    let format: AudioStreamBasicDescription = unsafe { read(object, &address)? };
    let flags = format.mFormatFlags;
    let packed = flags & kAudioFormatFlagIsPacked != 0;
    let float32 = flags & kAudioFormatFlagIsFloat != 0 && format.mBitsPerChannel == 32;
    let signed16 = flags & kAudioFormatFlagIsFloat == 0
        && flags & kAudioFormatFlagIsSignedInteger != 0
        && format.mBitsPerChannel == 16;
    let supported = format.mFormatID == kAudioFormatLinearPCM && packed && (float32 || signed16);
    (supported && format.mChannelsPerFrame > 0)
        .then_some((format.mChannelsPerFrame as u16, format.mSampleRate as u32))
}

fn supported_sample_rates(object: AudioObjectID, scope: AudioObjectPropertyScope) -> Vec<u32> {
    let address = address(kAudioDevicePropertyAvailableNominalSampleRates, scope);
    // SAFETY: available nominal rates are an array of `AudioValueRange`.
    let ranges: Vec<AudioValueRange> = unsafe { read_array(object, &address) }.unwrap_or_default();
    COMMON_SAMPLE_RATES
        .iter()
        .copied()
        .filter(|&rate| {
            ranges
                .iter()
                .any(|r| f64::from(rate) >= r.mMinimum && f64::from(rate) <= r.mMaximum)
        })
        .collect()
}

fn buffer_size_range(object: AudioObjectID) -> Option<BufferSizeRange> {
    let address = address(
        kAudioDevicePropertyBufferFrameSizeRange,
        kAudioObjectPropertyScopeGlobal,
    );
    // SAFETY: the buffer frame size range is an `AudioValueRange`.
    let range: AudioValueRange = unsafe { read(object, &address)? };
    (range.mMaximum > 0.0).then_some(BufferSizeRange {
        min: range.mMinimum as u32,
        max: range.mMaximum as u32,
    })
}

fn default_device(selector: AudioObjectPropertySelector) -> Option<AudioObjectID> {
    let address = address(selector, kAudioObjectPropertyScopeGlobal);
    // SAFETY: default device properties are an `AudioDeviceID`.
    unsafe { read(kAudioObjectSystemObject as AudioObjectID, &address) }
}

/// Enumerate CoreAudio devices with their capabilities, returning
/// `(outputs, inputs)`. A device with both directions appears in both lists.
/// Ids match cpal's `DeviceId` strings, so they resolve through cpal when a
/// stream is opened.
pub fn enumerate_devices(host_id: &str) -> (Vec<AudioDeviceInfo>, Vec<AudioDeviceInfo>) {
    let system = kAudioObjectSystemObject as AudioObjectID;
    let devices_address = address(
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
    );
    // SAFETY: the device list is an array of `AudioDeviceID`.
    let Some(devices) = (unsafe { read_array::<AudioObjectID>(system, &devices_address) }) else {
        return (Vec::new(), Vec::new());
    };
    let default_output = default_device(kAudioHardwarePropertyDefaultOutputDevice);
    let default_input = default_device(kAudioHardwarePropertyDefaultInputDevice);

    let mut outputs = Vec::new();
    let mut inputs = Vec::new();

    for device in devices {
        let output_format = default_format(device, kAudioObjectPropertyScopeOutput);
        let input_format = default_format(device, kAudioObjectPropertyScopeInput);
        if output_format.is_none() && input_format.is_none() {
            continue;
        }
        let Some(uid) = read_string(device, kAudioDevicePropertyDeviceUID) else {
            continue;
        };
        let Some(name) = read_string(device, kAudioObjectPropertyName) else {
            continue;
        };
        let id = cpal::DeviceId::new(cpal::HostId::CoreAudio, uid).to_string();
        let buffer_size_range = buffer_size_range(device);

        if let Some((channels, sample_rate)) = output_format {
            outputs.push(AudioDeviceInfo {
                id: id.clone(),
                host_id: host_id.to_string(),
                name: name.clone(),
                input_channels: 0,
                output_channels: channels,
                is_default: default_output == Some(device),
                sample_rate,
                supported_sample_rates: supported_sample_rates(
                    device,
                    kAudioObjectPropertyScopeOutput,
                ),
                buffer_size_range: buffer_size_range.clone(),
            });
        }

        if let Some((channels, sample_rate)) = input_format {
            inputs.push(AudioDeviceInfo {
                id,
                host_id: host_id.to_string(),
                name,
                input_channels: channels,
                output_channels: 0,
                is_default: default_input == Some(device),
                sample_rate,
                supported_sample_rates: supported_sample_rates(
                    device,
                    kAudioObjectPropertyScopeInput,
                ),
                buffer_size_range,
            });
        }
    }

    (outputs, inputs)
}
