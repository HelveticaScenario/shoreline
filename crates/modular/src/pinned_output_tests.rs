//! An output opened on a device that happens to be the system default stays
//! on it when the default moves; only System Default follows the move.
//!
//! The test moves the system default output between BlackHole 16ch and 2ch
//! and puts the original back when it ends, so it needs both installed and
//! briefly changes what other apps play through:
//! `cargo test -p modular --lib pinned_output -- --ignored`.

use std::mem;
use std::ptr::{NonNull, null};
use std::time::Duration;

use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress,
    AudioObjectPropertySelector, AudioObjectSetPropertyData,
    kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioHardwarePropertyTranslateUIDToDevice, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
};
use objc2_core_foundation::CFString;

use crate::device_follow::OutputEvent;
use crate::{AudioConfigOptions, Synthesizer};

const PINNED_UID: &str = "BlackHole16ch_UID";
const OTHER_UID: &str = "BlackHole2ch_UID";

fn address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// # Safety
/// `T` must be the property's data type, and `qualifier` the property's
/// qualifier type when it takes one.
unsafe fn read<T: Copy, Q>(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    qualifier: Option<&Q>,
) -> T {
    let address = address(selector);
    // SAFETY: zeroed is a valid placeholder for the plain C types read here.
    let mut value: T = unsafe { mem::zeroed() };
    let mut size = mem::size_of::<T>() as u32;
    let (qualifier_size, qualifier) = match qualifier {
        Some(q) => (mem::size_of::<Q>() as u32, (q as *const Q).cast()),
        None => (0, null()),
    };
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            qualifier_size,
            qualifier,
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    assert_eq!(status, 0, "reading CoreAudio property {selector} failed");
    value
}

fn device(uid: &str) -> AudioObjectID {
    let uid = CFString::from_str(uid);
    let uid: *const CFString = &*uid;
    // SAFETY: the translation takes a CFStringRef qualifier and yields an
    // AudioObjectID, 0 when no device has the UID.
    let device: AudioObjectID = unsafe {
        read(
            kAudioObjectSystemObject as AudioObjectID,
            kAudioHardwarePropertyTranslateUIDToDevice,
            Some(&uid),
        )
    };
    assert_ne!(device, 0, "{uid:?} is not installed");
    device
}

fn default_output() -> AudioObjectID {
    // SAFETY: the default output property is an AudioObjectID.
    unsafe {
        read::<_, ()>(
            kAudioObjectSystemObject as AudioObjectID,
            kAudioHardwarePropertyDefaultOutputDevice,
            None,
        )
    }
}

fn set_default_output(device: AudioObjectID) {
    let address = address(kAudioHardwarePropertyDefaultOutputDevice);
    // SAFETY: the default output property is an AudioObjectID.
    let status = unsafe {
        AudioObjectSetPropertyData(
            kAudioObjectSystemObject as AudioObjectID,
            NonNull::from(&address),
            0,
            null(),
            mem::size_of::<AudioObjectID>() as u32,
            NonNull::from(&device).cast(),
        )
    };
    assert_eq!(status, 0, "setting the default output failed");
}

fn is_running(device: AudioObjectID) -> bool {
    // SAFETY: the running property is a u32 flag.
    unsafe { read::<u32, ()>(device, kAudioDevicePropertyDeviceIsRunningSomewhere, None) != 0 }
}

/// Puts the system default output back however the test ends.
struct RestoreDefault(AudioObjectID);

impl Drop for RestoreDefault {
    fn drop(&mut self) {
        set_default_output(self.0);
    }
}

#[test]
#[ignore = "moves the system default output between BlackHole 16ch and 2ch"]
fn a_device_that_is_the_default_stays_put_when_the_default_moves() {
    let pinned = device(PINNED_UID);
    let other = device(OTHER_UID);
    let _restore = RestoreDefault(default_output());
    set_default_output(pinned);
    std::thread::sleep(Duration::from_millis(500));

    let device_id = cpal::DeviceId::new(cpal::HostId::CoreAudio, PINNED_UID).to_string();
    let mut synth = Synthesizer::new(Some(AudioConfigOptions {
        output_device_id: Some(device_id.clone()),
        input_device_id: Some("none".into()),
        ..Default::default()
    }))
    .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(is_running(pinned), "the output does not open on its device");
    assert!(!is_running(other), "BlackHole 2ch is in use elsewhere");

    set_default_output(other);
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        synth.follow_output_device().is_none(),
        "a pinned output reacts to the default moving"
    );
    assert!(is_running(pinned), "the output left its device");
    assert!(!is_running(other), "the output followed the default");
    assert_eq!(synth.get_output_device_id(), Some(device_id));
}

#[test]
#[ignore = "moves the system default output between BlackHole 16ch and 2ch"]
fn system_default_moves_with_the_default() {
    let first = device(PINNED_UID);
    let next = device(OTHER_UID);
    let _restore = RestoreDefault(default_output());
    set_default_output(first);
    std::thread::sleep(Duration::from_millis(500));

    let mut synth = Synthesizer::new(Some(AudioConfigOptions {
        output_device_id: None,
        input_device_id: Some("none".into()),
        ..Default::default()
    }))
    .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(is_running(first), "the output does not open on the default");

    set_default_output(next);
    std::thread::sleep(Duration::from_millis(500));
    let report = synth
        .follow_output_device()
        .expect("System Default does not follow the default");
    assert_eq!(report.event, OutputEvent::Moved);
    assert!(report.engine_kept, "the move does not keep the engine");
    std::thread::sleep(Duration::from_millis(500));
    assert!(is_running(next), "the output is not on the new default");
    assert!(!is_running(first), "the output stays on the old default");
}
