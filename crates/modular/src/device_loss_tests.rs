//! A stream whose device disappears reopens once the device returns, with the
//! engine it was playing and what was sent to it meanwhile, and a device
//! missing at launch is opened once it arrives.
//!
//! The device is a process-private aggregate wrapping BlackHole 64ch, so it
//! can be unplugged and replugged under the same UID without touching any
//! device other processes see. Needs BlackHole installed:
//! `cargo test -p modular --lib device_loss -- --ignored`.

use std::ffi::CStr;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use objc2_core_audio::{
    AudioHardwareCreateAggregateDevice, AudioHardwareDestroyAggregateDevice, AudioObjectID,
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceSubDeviceListKey, kAudioAggregateDeviceUIDKey, kAudioSubDeviceUIDKey,
};
use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType};

use crate::device_poll::{DeviceEvent, DeviceReport};
use modular_core::types::{ModuleSpec, PatchGraph};

use crate::{AudioConfigOptions, Synthesizer};

const BLACKHOLE_UID: &str = "BlackHole64ch_UID";
const AGGREGATE_UID: &str = "shoreline.test.device-loss";

fn key(name: &CStr) -> CFRetained<CFString> {
    CFString::from_str(name.to_str().unwrap())
}

fn plug() -> AudioObjectID {
    let sub_device = CFDictionary::<CFString, CFType>::from_slices(
        &[&key(kAudioSubDeviceUIDKey)],
        &[&CFString::from_str(BLACKHOLE_UID)],
    );
    let sub_devices = CFArray::from_objects(&[&*sub_device]);
    let private = CFNumber::new_i32(1);
    let description = CFDictionary::<CFString, CFType>::from_slices(
        &[
            &key(kAudioAggregateDeviceUIDKey),
            &key(kAudioAggregateDeviceNameKey),
            &key(kAudioAggregateDeviceIsPrivateKey),
            &key(kAudioAggregateDeviceSubDeviceListKey),
        ],
        &[
            &CFString::from_str(AGGREGATE_UID),
            &CFString::from_str("Shoreline device-loss test"),
            &private,
            &sub_devices,
        ],
    );
    let mut device: AudioObjectID = 0;
    // SAFETY: the description is a CFDictionary of CFString keys, and `device`
    // outlives the call.
    let status = unsafe {
        AudioHardwareCreateAggregateDevice(description.as_opaque(), NonNull::from(&mut device))
    };
    assert_eq!(status, 0, "creating the aggregate device failed");
    device
}

fn unplug(device: AudioObjectID) {
    // SAFETY: `device` is an aggregate this process created.
    assert_eq!(unsafe { AudioHardwareDestroyAggregateDevice(device) }, 0);
}

fn poll_until(
    synth: &mut Synthesizer,
    want: DeviceEvent,
    within: Duration,
) -> Option<DeviceReport> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if let Some(report) = synth.poll_devices()
            && report.event == want
        {
            return Some(report);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

#[test]
#[ignore = "needs BlackHole 64ch installed"]
fn an_unplugged_device_reopens_with_its_engine_when_it_returns() {
    let device = plug();
    // CoreAudio publishes a new device shortly after creating it.
    std::thread::sleep(Duration::from_millis(500));
    let device_id = cpal::DeviceId::new(cpal::HostId::CoreAudio, AGGREGATE_UID).to_string();
    let mut synth = Synthesizer::new(Some(AudioConfigOptions {
        output_device_id: Some(device_id.clone()),
        input_device_id: Some("none".into()),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(synth.streams.output_device_id, device_id.clone());
    assert!(
        synth.poll_devices().is_none(),
        "a running stream reads as changed"
    );
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        synth.poll_devices().is_none(),
        "a running stream reads as stopped"
    );

    unplug(device);
    let lost = poll_until(
        &mut synth,
        DeviceEvent::Disconnected,
        Duration::from_secs(5),
    )
    .expect("an unplugged device is not noticed");
    assert_eq!(
        lost.output_device_name.as_deref(),
        Some("Shoreline device-loss test")
    );
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        synth.poll_devices().is_none(),
        "a lost device is reported again"
    );

    let device = plug();
    let back = poll_until(&mut synth, DeviceEvent::Reconnected, Duration::from_secs(5))
        .expect("a replugged device is not reopened");
    assert!(!back.engine_reset, "the engine does not survive the reopen");
    assert_eq!(synth.streams.output_device_id, device_id);
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        synth.poll_devices().is_none(),
        "the reopened stream does not run"
    );

    drop(synth);
    unplug(device);
}

#[test]
#[ignore = "needs BlackHole 64ch installed"]
fn a_device_missing_at_launch_is_kept_as_the_choice_and_opened_when_it_arrives() {
    let device_id = cpal::DeviceId::new(cpal::HostId::CoreAudio, AGGREGATE_UID).to_string();
    let mut synth = Synthesizer::new(Some(AudioConfigOptions {
        output_device_id: Some(device_id.clone()),
        input_device_id: Some("none".into()),
        ..Default::default()
    }))
    .unwrap();
    assert_ne!(synth.streams.output_device_id, device_id.clone());
    let state = synth.get_current_audio_state();
    assert_eq!(state.chosen_output_device_id, Some(device_id.clone()));

    let device = plug();
    let arrived = poll_until(&mut synth, DeviceEvent::Connected, Duration::from_secs(8))
        .expect("the chosen device is not opened when it arrives");
    assert_eq!(
        arrived.output_device_name.as_deref(),
        Some("Shoreline device-loss test")
    );
    assert_eq!(synth.streams.output_device_id, device_id.clone());
    assert_eq!(
        synth.get_current_audio_state().chosen_output_device_id,
        Some(device_id)
    );

    drop(synth);
    unplug(device);
}

fn signal_patch(value: f64) -> PatchGraph {
    PatchGraph {
        modules: vec![ModuleSpec {
            id: "s".into(),
            module_type: "$signal".into(),
            id_is_explicit: None,
            skip_state_transfer: None,
            params: serde_json::json!({ "min": 0, "max": 5, "value": value }),
        }],
        module_id_remaps: None,
        scopes: vec![],
        vu_meters: vec![],
    }
}

#[test]
#[ignore = "needs BlackHole 64ch installed"]
fn updates_sent_during_a_long_disconnect_play_once_the_device_returns() {
    let device = plug();
    std::thread::sleep(Duration::from_millis(500));
    let device_id = cpal::DeviceId::new(cpal::HostId::CoreAudio, AGGREGATE_UID).to_string();
    let mut synth = Synthesizer::new(Some(AudioConfigOptions {
        output_device_id: Some(device_id),
        input_device_id: Some("none".into()),
        ..Default::default()
    }))
    .unwrap();
    assert!(
        synth
            .update_patch(signal_patch(0.0), None, None)
            .errors
            .is_empty()
    );
    std::thread::sleep(Duration::from_millis(500));

    unplug(device);
    poll_until(
        &mut synth,
        DeviceEvent::Disconnected,
        Duration::from_secs(5),
    )
    .expect("an unplugged device is not noticed");
    // Minutes of a video region's level at 60 frames a second.
    for frame in 0..10_000 {
        synth
            .set_module_param(
                "s".into(),
                "$signal".into(),
                serde_json::json!({ "min": 0, "max": 5, "value": frame as f64 / 10_000.0 }),
            )
            .expect("a module update during the disconnect fails");
    }
    let update = synth.update_patch(signal_patch(1.0), None, None);
    assert!(
        update.errors.is_empty(),
        "a patch update during the disconnect fails"
    );

    let device = plug();
    poll_until(&mut synth, DeviceEvent::Reconnected, Duration::from_secs(5))
        .expect("a replugged device is not reopened");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        synth.get_transport_state().last_applied_update_id,
        update.update_id,
        "the patch update sent during the disconnect does not play"
    );

    drop(synth);
    unplug(device);
}
