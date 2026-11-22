//! The Linux hosts against running sound servers: every listed output plays,
//! and System Default follows the default sink.
//!
//! Each test needs its server running with at least two sinks, and changes
//! the default sink: `cargo test -p modular --lib linux_hosts -- --ignored
//! pipewire` with PipeWire and pipewire-pulse, `pulseaudio` with a
//! PulseAudio daemon, `jack` with a JACK server. `preferred` checks the host
//! a synthesizer opens on unasked against `SHORELINE_EXPECT_HOST`.

use std::process::Command;
use std::time::{Duration, Instant};

use crate::audio::{AudioDeviceInfo, get_host_by_preference, host_key};
use crate::device_poll::DeviceEvent;
use crate::{AudioConfigOptions, Synthesizer};

const SETTLE: Duration = Duration::from_millis(600);

/// How long a new stream may take to deliver its first callbacks: a waking
/// PulseAudio sink takes over a second. Below the watchdog's stall limit, so
/// a stream that passes is one the engine keeps.
const FIRST_CALLBACK: Duration = Duration::from_millis(2500);

fn no_input() -> Option<String> {
    Some("none".into())
}

/// A synthesizer on `host`'s System Default, or the preferred host's.
fn open(host: Option<&str>) -> Synthesizer {
    let synth = Synthesizer::new(Some(AudioConfigOptions {
        host_id: host.map(str::to_owned),
        input_device_id: no_input(),
        ..Default::default()
    }))
    .unwrap_or_else(|err| panic!("{host:?}: no synthesizer opens: {err}"));
    assert_playing(&synth, "System Default");
    synth
}

/// Asserts the output stream delivers callbacks.
fn assert_playing(synth: &Synthesizer, what: &str) {
    let health = &synth.streams.output_health;
    let before = health.callbacks();
    let deadline = Instant::now() + FIRST_CALLBACK;
    while health.callbacks() == before {
        assert!(
            Instant::now() < deadline,
            "{what}: the output stream runs no callbacks"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!health.has_ended(), "{what}: the output stream ended");
}

/// Reopens the streams on `output` as given and asserts they play.
fn play_on(
    synth: &mut Synthesizer,
    output: &AudioDeviceInfo,
    sample_rate: u32,
    buffer_size: Option<u32>,
    input: Option<&AudioDeviceInfo>,
) {
    let what = format!(
        "{} at {sample_rate} Hz, {buffer_size:?} frames, input {:?}",
        output.id,
        input.map(|d| &d.id)
    );
    synth
        .recreate_streams(
            Some(output.id.clone()),
            sample_rate,
            buffer_size,
            input.map(|d| d.id.clone()).or_else(no_input),
            None,
        )
        .unwrap_or_else(|err| panic!("{what}: {err}"));
    assert_eq!(synth.streams.output_device_id, output.id, "{what}");
    assert_eq!(synth.streams.sample_rate, sample_rate, "{what}");
    assert_eq!(synth.streams.buffer_size, buffer_size, "{what}");
    if let Some(input) = input.filter(|d| d.supported_sample_rates.contains(&sample_rate)) {
        assert_eq!(
            synth.streams.input_device_id.as_deref(),
            Some(input.id.as_str()),
            "{what}: the input does not open"
        );
    }
    assert_playing(synth, &what);
}

/// Opens the host, then every output it lists at every listed rate, at a
/// fixed buffer size, and with every listed input beside it.
fn every_output_plays(host: &str) {
    let mut synth = open(Some(host));
    assert_eq!(synth.streams.host_id, host, "opened on another host");

    let outputs = synth.device_cache.output_devices_for_host(host);
    let inputs = synth.device_cache.input_devices_for_host(host);
    eprintln!("{host} outputs: {outputs:#?}\n{host} inputs: {inputs:#?}");
    assert!(!outputs.is_empty(), "{host} lists no outputs");
    for devices in [&outputs, &inputs] {
        assert!(
            devices.iter().filter(|d| d.is_default).count() <= 1,
            "{host} lists more than one default"
        );
        let mut ids: Vec<_> = devices.iter().map(|d| &d.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), devices.len(), "{host} lists a device twice");
        assert!(
            devices.iter().all(|d| d.is_default || !d.follows_default),
            "{host} lists a follower that is not the default"
        );
        assert_eq!(
            devices.iter().any(|d| d.follows_default),
            host == "PipeWire",
            "{host} default follower"
        );
    }
    for output in &outputs {
        assert!(!output.name.is_empty(), "{output:?} has no name");
        assert!(output.output_channels > 0, "{output:?} has no channels");
        assert!(
            output.supported_sample_rates.contains(&output.sample_rate),
            "{output:?} does not support its own rate"
        );
        for &rate in &output.supported_sample_rates {
            play_on(&mut synth, output, rate, None, None);
        }
        if let Some(range) = &output.buffer_size_range {
            let frames = range.min.max(256).min(range.max);
            play_on(&mut synth, output, output.sample_rate, Some(frames), None);
        }
        for input in &inputs {
            play_on(&mut synth, output, output.sample_rate, None, Some(input));
        }
    }
}

fn pactl(args: &[&str]) -> String {
    let out = Command::new("pactl")
        .args(args)
        .output()
        .expect("pactl does not run");
    assert!(out.status.success(), "pactl {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The first two tab-separated columns of each line of a `pactl list short`.
fn short_list(what: &str) -> Vec<(String, String)> {
    pactl(&["list", "short", what])
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            Some((columns.next()?.to_owned(), columns.next()?.to_owned()))
        })
        .collect()
}

/// The names of the sinks the server's playback streams play on.
fn sinks_in_use() -> Vec<String> {
    let sinks = short_list("sinks");
    short_list("sink-inputs")
        .into_iter()
        .filter_map(|(_, sink)| sinks.iter().find(|(index, _)| *index == sink))
        .map(|(_, name)| name.clone())
        .collect()
}

fn default_sink() -> String {
    pactl(&["get-default-sink"]).trim().to_owned()
}

/// Makes a sink other than the current default the default, and returns it.
fn move_default_sink() -> String {
    let default = default_sink();
    let (_, next) = short_list("sinks")
        .into_iter()
        .find(|(_, name)| *name != default)
        .expect("the server has only one sink");
    pactl(&["set-default-sink", &next]);
    std::thread::sleep(SETTLE * 2);
    next
}

#[test]
#[ignore = "needs PipeWire and pipewire-pulse with two sinks"]
fn pipewire_every_output_plays() {
    every_output_plays("PipeWire");
}

#[test]
#[ignore = "needs PipeWire and pipewire-pulse with two sinks; changes the default sink"]
fn pipewire_system_default_follows_the_default_sink() {
    let mut synth = open(Some("PipeWire"));
    let output = synth
        .device_cache
        .find_output_device(&synth.streams.output_device_id)
        .expect("System Default is not listed");
    assert!(
        output.follows_default,
        "{output:?} does not follow the default"
    );
    assert_eq!(output.name, "System Default");
    let next = move_default_sink();
    assert!(
        synth.poll_devices().is_none(),
        "the stream follows the default itself"
    );
    assert_eq!(sinks_in_use(), [next], "the output stays behind");
    assert_playing(&synth, "System Default after the move");
}

#[test]
#[ignore = "needs PipeWire and pipewire-pulse with two sinks; changes the default sink"]
fn pipewire_a_chosen_output_stays_when_the_default_moves() {
    let mut synth = open(Some("PipeWire"));
    let default = default_sink();
    let chosen = synth
        .device_cache
        .find_output_device(&format!("pipewire:{default}"))
        .expect("the default sink is not listed under its node name")
        .clone();
    play_on(&mut synth, &chosen, chosen.sample_rate, None, None);
    move_default_sink();
    assert!(synth.poll_devices().is_none());
    assert_eq!(sinks_in_use(), [default], "the chosen output moved");
    assert_playing(&synth, "the chosen output");
}

#[test]
#[ignore = "needs a PulseAudio server with two sinks"]
fn pulseaudio_every_output_plays() {
    every_output_plays("PulseAudio");
}

#[test]
#[ignore = "needs a PulseAudio server with two sinks; changes the default sink"]
fn pulseaudio_system_default_follows_the_default_sink() {
    let mut synth = open(Some("PulseAudio"));
    assert!(synth.poll_devices().is_none());
    let clients = short_list("clients").len();
    for _ in 0..3 {
        assert!(synth.poll_devices().is_none());
    }
    assert_eq!(
        short_list("clients").len(),
        clients,
        "polling connects new clients"
    );
    let next = move_default_sink();
    let report = synth
        .poll_devices()
        .expect("System Default does not follow the default sink");
    assert_eq!(report.event, DeviceEvent::Moved);
    assert_eq!(synth.streams.output_device_id, format!("pulseaudio:{next}"));
    std::thread::sleep(SETTLE);
    assert_eq!(
        sinks_in_use(),
        [next],
        "the output is not on the new default"
    );
    assert_playing(&synth, "System Default after the move");
}

#[test]
#[ignore = "needs a JACK server"]
fn jack_every_output_plays() {
    every_output_plays("Jack");
}

#[test]
#[ignore = "needs SHORELINE_EXPECT_HOST naming the host this machine prefers"]
fn preferred_host() {
    let expected = std::env::var("SHORELINE_EXPECT_HOST").expect("SHORELINE_EXPECT_HOST");
    assert_eq!(host_key(get_host_by_preference().id()), expected);
    assert_eq!(open(None).streams.host_id, expected);
}
