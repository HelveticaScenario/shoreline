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
    AudioObjectID, AudioObjectSetPropertyData, kAudioDevicePropertyDeviceIsRunningSomewhere,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyTranslateUIDToDevice,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
};
use objc2_core_foundation::CFString;

use crate::coreaudio_devices::{address, default_device, read_qualified};
use crate::device_poll::DeviceEvent;
use crate::{AudioConfigOptions, Synthesizer};

const PINNED_UID: &str = "BlackHole16ch_UID";
const OTHER_UID: &str = "BlackHole2ch_UID";

fn device(uid: &str) -> AudioObjectID {
    let uid = CFString::from_str(uid);
    let uid: *const CFString = &*uid;
    let address = address(
        kAudioHardwarePropertyTranslateUIDToDevice,
        kAudioObjectPropertyScopeGlobal,
    );
    // SAFETY: the translation takes a CFStringRef qualifier and yields an
    // AudioObjectID, 0 when no device has the UID.
    let device: AudioObjectID = unsafe {
        read_qualified(
            kAudioObjectSystemObject as AudioObjectID,
            &address,
            Some(&uid),
        )
    }
    .expect("translating a UID failed");
    assert_ne!(device, 0, "{uid:?} is not installed");
    device
}

fn default_output() -> AudioObjectID {
    default_device(kAudioHardwarePropertyDefaultOutputDevice).expect("no default output")
}

fn set_default_output(device: AudioObjectID) {
    let address = address(
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
    );
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
    let address = address(
        kAudioDevicePropertyDeviceIsRunningSomewhere,
        kAudioObjectPropertyScopeGlobal,
    );
    // SAFETY: the running property is a u32 flag.
    unsafe { read_qualified::<u32, ()>(device, &address, None) }.expect("reading IsRunning failed")
        != 0
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
        synth.poll_devices().is_none(),
        "a pinned output reacts to the default moving"
    );
    assert!(is_running(pinned), "the output left its device");
    assert!(!is_running(other), "the output followed the default");
    assert_eq!(synth.streams.output_device_id, device_id);
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
        .poll_devices()
        .expect("System Default does not follow the default");
    assert_eq!(report.event, DeviceEvent::Moved);
    assert!(!report.engine_reset, "the move does not keep the engine");
    std::thread::sleep(Duration::from_millis(500));
    assert!(is_running(next), "the output is not on the new default");
    assert!(!is_running(first), "the output stays on the old default");
}
