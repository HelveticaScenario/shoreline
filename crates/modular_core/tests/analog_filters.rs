//! `$lpf`, `$hpf`, `$bpf`, `$notch` and their Sallen-Key, ladder, diode,
//! OTA and Wasp counterparts: each output has its response, the analog
//! models self-oscillate in tune, or for the Wasp resonate in tune short of
//! it, and stay bounded, and an output renders the same whichever others
//! are read.

use modular_core::dsp::schema;
use modular_core::dsp::utils::rng::TEST_SEED_BASE;
use serde_json::{Value, json};

mod block_common;
mod common;
mod drum_common;
use block_common::{LANES, check_connections_agree, poly_cable};
use common::{finite_peak, render_ports};
use drum_common::voct as hz;

const SAMPLE_RATE: f32 = 48_000.0;

fn cable(module: &str) -> Value {
    block_common::cable(module, 0)
}

fn freq_is_center(module_type: &str) -> bool {
    module_type.starts_with("$bpf") || module_type.starts_with("$notch")
}

fn freq_param(module_type: &str) -> &'static str {
    if freq_is_center(module_type) {
        "center"
    } else {
        "cutoff"
    }
}

/// `frames` samples of every output of `module_type`, by port, filtering a
/// sine of `level` volts at `input_hz`.
fn render(
    module_type: &str,
    input_hz: f32,
    level: f32,
    cutoff: Value,
    resonance: f32,
    frames: usize,
) -> Vec<(&'static str, Vec<f32>)> {
    render_with(
        module_type,
        input_hz,
        level,
        cutoff,
        resonance,
        frames,
        json!({}),
    )
}

/// [`render`], with the fields of `extra` among the filter's params.
fn render_with(
    module_type: &str,
    input_hz: f32,
    level: f32,
    cutoff: Value,
    resonance: f32,
    frames: usize,
    extra: Value,
) -> Vec<(&'static str, Vec<f32>)> {
    let mut params = json!({ "input": cable("level"), "resonance": resonance });
    params[freq_param(module_type)] = cutoff;
    for (name, value) in extra.as_object().unwrap() {
        params[name] = value.clone();
    }
    let modules = vec![
        ("mod", "$sine", json!({ "freq": 0.0 })),
        ("src", "$sine", json!({ "freq": hz(input_hz) })),
        (
            "level",
            "$mul",
            json!({ "input": cable("src"), "factor": level / 5.0 }),
        ),
        ("filt", module_type, params),
    ];
    render_ports(modules, SAMPLE_RATE, frames, "filt")
}

/// Peak of the second half of `samples`, once the filter has settled.
fn peak(samples: &[f32]) -> f32 {
    samples[samples.len() / 2..]
        .iter()
        .fold(0.0f32, |a, v| a.max(v.abs()))
}

/// Frequency of the second half of `samples`, from its rising zero
/// crossings.
fn frequency(samples: &[f32]) -> f32 {
    let tail = &samples[samples.len() / 2..];
    let crossings: Vec<f32> = tail
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
        .map(|(i, w)| i as f32 + w[0] / (w[0] - w[1]))
        .collect();
    if crossings.len() < 2 {
        return 0.0;
    }
    (crossings.len() - 1) as f32 * SAMPLE_RATE / (crossings[crossings.len() - 1] - crossings[0])
}

const CLEAN: [&str; 4] = ["$lpf", "$hpf", "$bpf", "$notch"];
const ANALOG: [&str; 18] = [
    "$lpfSk",
    "$hpfSk",
    "$bpfSk",
    "$notchSk",
    "$lpfLadder",
    "$hpfLadder",
    "$bpfLadder",
    "$notchLadder",
    "$apfLadder",
    "$lpfDiode",
    "$hpfDiode",
    "$bpfDiode",
    "$notchDiode",
    "$lpfOta",
    "$hpfOta",
    "$bpfOta",
    "$notchOta",
    "$apfOta",
];

/// The Wasp filters, which have the one slope and never self-oscillate.
const WASP: [&str; 5] = ["$lpfWasp", "$hpfWasp", "$bpfWasp", "$notchWasp", "$apfWasp"];

/// Every filter with both a 12 and a 24 dB output.
fn all() -> impl Iterator<Item = &'static str> {
    CLEAN.into_iter().chain(ANALOG)
}

/// Gain in dB of every output, in slope order, for a quiet sine `ratio` ×
/// the 1 kHz cutoff at resonance 0.
fn gain_db(module_type: &str, ratio: f32) -> Vec<f32> {
    gain_db_at(module_type, ratio, 0.0)
}

/// [`gain_db`] at `resonance`.
fn gain_db_at(module_type: &str, ratio: f32, resonance: f32) -> Vec<f32> {
    render(
        module_type,
        1000.0 * ratio,
        0.1,
        json!(hz(1000.0)),
        resonance,
        9600,
    )
    .into_iter()
    .map(|(_, samples)| 20.0 * (peak(&samples) / 0.1).log10())
    .collect()
}

#[test]
fn each_steeper_output_cuts_more() {
    for module_type in all() {
        let (stopband, passband): (&[f32], f32) = match &module_type[..4] {
            "$lpf" => (&[8.0], 0.125),
            "$hpf" => (&[0.125], 8.0),
            "$bpf" => (&[0.125, 8.0], 1.0),
            _ => continue,
        };
        for &ratio in stopband {
            let gains = gain_db(module_type, ratio);
            assert!(gains[0] < -3.0, "{module_type} at {ratio}×: {gains:?}");
            for pair in gains.windows(2) {
                assert!(
                    pair[1] < pair[0] - 4.5,
                    "{module_type} at {ratio}×: {gains:?}"
                );
            }
        }
        for db in gain_db(module_type, passband) {
            assert!(db.abs() < 4.5, "{module_type} passes its band at {db} dB");
        }
    }
}

#[test]
fn the_clean_24db_outputs_are_butterworth_flat() {
    for (module_type, passband) in [("$lpf", 0.5), ("$hpf", 2.0)] {
        let at_cutoff = gain_db(module_type, 1.0)[1];
        let inside = gain_db(module_type, passband)[1];
        assert!(
            (at_cutoff + 3.0).abs() < 0.2,
            "{module_type}: {at_cutoff} dB"
        );
        assert!(inside.abs() < 0.2, "{module_type}: {inside} dB");
    }
}

#[test]
fn notches_null_at_the_centre_or_half_an_octave_either_side() {
    for module_type in all().filter(|module_type| module_type.starts_with("$notch")) {
        let centre = gain_db(module_type, 1.0);
        assert!(centre[0] < -25.0, "{module_type} 12db: {} dB", centre[0]);
        for ratio in [0.5f32.sqrt(), 2.0f32.sqrt()] {
            let null = gain_db(module_type, ratio)[1];
            assert!(null < -25.0, "{module_type} 24db at {ratio}×: {null} dB");
        }
        // Either side of the cut plays at one level.
        for ratio in [1.0 / 32.0, 32.0] {
            for db in gain_db(module_type, ratio) {
                assert!(db.abs() < 3.5, "{module_type} at {ratio}×: {db} dB");
            }
        }
    }
}

#[test]
fn allpasses_keep_the_level() {
    for module_type in ["$apfLadder", "$apfOta"] {
        for ratio in [0.125, 0.5, 1.0, 2.0, 8.0] {
            for db in gain_db(module_type, ratio) {
                assert!(db.abs() < 1.0, "{module_type} at {ratio}×: {db} dB");
            }
        }
    }
}

#[test]
fn allpasses_keep_the_level_at_any_resonance() {
    for module_type in ["$apfLadder", "$apfOta"] {
        for resonance in [2.0, 4.0] {
            // The 12 dB output peaks at the cutoff; away from it, and
            // everywhere on the 24 dB output, the level holds.
            for ratio in [1.0 / 32.0, 32.0] {
                for db in gain_db_at(module_type, ratio, resonance) {
                    assert!(db.abs() < 1.0, "{module_type} at {ratio}×: {db} dB");
                }
            }
            for ratio in [0.5, 1.0, 2.0] {
                let db = gain_db_at(module_type, ratio, resonance)[1];
                assert!(db.abs() < 1.0, "{module_type} 24db at {ratio}×: {db} dB");
            }
        }
    }
}

#[test]
fn a_loud_input_passes_an_allpass_at_its_own_level() {
    for module_type in ["$apfLadder", "$apfOta"] {
        let out = render(module_type, 30.0, 5.0, json!(hz(1000.0)), 4.0, 9600);
        for (port, samples) in out {
            let db = 20.0 * (peak(&samples) / 5.0).log10();
            assert!(db.abs() < 1.0, "{module_type} {port}: {db} dB");
        }
    }
}

#[test]
fn analog_filters_self_oscillate_in_tune() {
    // A notch, and the 24 dB allpass, null their own oscillation, leaving
    // the harmonics.
    for module_type in ANALOG
        .into_iter()
        .filter(|module_type| !module_type.starts_with("$notch"))
    {
        for cutoff in [250.0, 1000.0, 4000.0] {
            let out = render(module_type, 110.0, 0.001, json!(hz(cutoff)), 5.0, 48_000);
            let nulled = |port: &str| module_type.starts_with("$apf") && port == "24db";
            for (port, samples) in out.into_iter().filter(|(port, _)| !nulled(port)) {
                let (volts, freq) = (peak(&samples), frequency(&samples));
                assert!(
                    (0.2..16.0).contains(&volts) && (freq / cutoff - 1.0).abs() < 0.025,
                    "{module_type} {port} at {cutoff} Hz: {volts} V at {freq} Hz"
                );
            }
        }
    }
}

/// Gain in dB of a Wasp filter at 1 kHz for a sine of `level` volts
/// `ratio` × that.
fn wasp_gain_db(module_type: &str, ratio: f32, level: f32, resonance: f32) -> f32 {
    let out = render(
        module_type,
        1000.0 * ratio,
        level,
        json!(hz(1000.0)),
        resonance,
        19_200,
    );
    20.0 * (peak(&out[0].1) / level).log10()
}

#[test]
fn wasp_filters_have_their_responses() {
    for (module_type, stopband, passband) in [
        ("$lpfWasp", vec![8.0], 0.125),
        ("$hpfWasp", vec![0.125], 7.3),
        ("$bpfWasp", vec![0.125, 8.0], 1.0),
    ] {
        for ratio in stopband {
            let db = wasp_gain_db(module_type, ratio, 0.1, 0.0);
            let slope = if module_type == "$bpfWasp" {
                -15.0
            } else {
                -30.0
            };
            assert!(db < slope, "{module_type} at {ratio}×: {db} dB");
        }
        let db = wasp_gain_db(module_type, passband, 0.1, 0.0);
        assert!(db.abs() < 1.0, "{module_type} passes its band at {db} dB");
    }
    let null = wasp_gain_db("$notchWasp", 1.0, 0.1, 0.0);
    assert!(null < -25.0, "$notchWasp: {null} dB");
    for ratio in [1.0 / 32.0, 32.0] {
        let db = wasp_gain_db("$notchWasp", ratio, 0.1, 0.0);
        assert!(db.abs() < 1.0, "$notchWasp at {ratio}×: {db} dB");
    }
    // Toward full resonance the allpass dips at the cutoff.
    for resonance in [0.0, 2.5, 4.0] {
        for ratio in [1.0 / 32.0, 0.5, 1.0, 2.0, 32.0] {
            let db = wasp_gain_db("$apfWasp", ratio, 0.1, resonance);
            assert!(
                db.abs() < 1.0,
                "$apfWasp at {ratio}×, resonance {resonance}: {db} dB"
            );
        }
    }
}

#[test]
fn wasp_resonance_peaks_in_tune_and_squashes_as_it_gets_loud() {
    for cutoff in [250.0, 1000.0, 4000.0] {
        let gain = |ratio: f32| {
            let out = render(
                "$bpfWasp",
                cutoff * ratio,
                0.01,
                json!(hz(cutoff)),
                5.0,
                48_000,
            );
            peak(&out[0].1) / 0.01
        };
        let (below, at, above) = (gain(0.97), gain(1.0), gain(1.03));
        assert!(
            at > 8.0 && at > 1.1 * below && at > 1.1 * above,
            "{cutoff} Hz: {below}× {at}× {above}×"
        );
    }
    let quiet = wasp_gain_db("$bpfWasp", 1.0, 0.01, 5.0);
    let loud = wasp_gain_db("$bpfWasp", 1.0, 2.0, 5.0);
    assert!(loud < quiet - 6.0, "{quiet} dB quiet, {loud} dB loud");
}

#[test]
fn wasp_filters_never_self_oscillate() {
    for module_type in WASP {
        for cutoff in [100.0, 1000.0, 8000.0] {
            let out = render(module_type, 30.0, 0.001, json!(hz(cutoff)), 5.0, 96_000);
            let volts = peak(&out[0].1);
            assert!(volts < 0.05, "{module_type} at {cutoff} Hz: {volts} V");
        }
    }
}

#[test]
fn wasp_filters_stay_bounded_when_driven_under_audio_rate_modulation() {
    for module_type in WASP {
        for (resonance, drive) in [(0.0, 0.0), (5.0, 0.0), (2.5, 5.0), (5.0, 5.0)] {
            let out = render_with(
                module_type,
                110.0,
                5.0,
                cable("mod"),
                resonance,
                24_000,
                json!({ "drive": drive }),
            );
            let peak = finite_peak(&out[0].1);
            assert!(
                peak < 40.0,
                "{module_type} resonance {resonance} drive {drive}: {peak} V"
            );
        }
    }
}

/// Peak in volts of a lowpass's default output passing a 100 Hz sine of
/// `level` volts well below its cutoff.
fn passband_peak(module_type: &str, level: f32, extra: Value) -> f32 {
    let out = render_with(
        module_type,
        100.0,
        level,
        json!(hz(4000.0)),
        0.0,
        9600,
        extra,
    );
    let port = block_common::default_port(module_type);
    peak(&out.iter().find(|(name, _)| *name == port).unwrap().1)
}

#[test]
fn drive_saturates_and_gives_back_half_its_gain() {
    for module_type in ["$lpfSk", "$lpfLadder", "$lpfDiode", "$lpfOta"] {
        // A quiet signal stays linear: +18 dB in, 9 dB back off.
        let quiet = passband_peak(module_type, 0.01, json!({ "drive": 5.0 }))
            / passband_peak(module_type, 0.01, json!({}));
        assert!(
            (20.0 * quiet.log10() - 9.03).abs() < 0.5,
            "{module_type}: {quiet}×"
        );
    }
    // A loud one runs into the stages' saturation and gains far less.
    for module_type in ["$lpfLadder", "$lpfDiode"] {
        let loud = passband_peak(module_type, 5.0, json!({ "drive": 5.0 }))
            / passband_peak(module_type, 5.0, json!({}));
        assert!(loud < 1.7, "{module_type}: {loud}×");
    }
}

#[test]
fn a_driven_sallen_key_passes_a_loud_input_unlimited_and_stays_bounded() {
    // Only the feedback clips, so drive raises a loud input as it does a
    // quiet one: +18 dB in, 9 dB back off.
    let loud = passband_peak("$lpfSk", 5.0, json!({ "drive": 5.0 }))
        / passband_peak("$lpfSk", 5.0, json!({}));
    assert!((20.0 * loud.log10() - 9.03).abs() < 1.0, "{loud}×");
    for module_type in ["$lpfSk", "$hpfSk", "$bpfSk", "$notchSk"] {
        let out = render_with(
            module_type,
            110.0,
            5.0,
            cable("mod"),
            5.0,
            24_000,
            json!({ "drive": 5.0 }),
        );
        for (port, samples) in out {
            let peak = finite_peak(&samples);
            assert!(peak < 200.0, "{module_type} {port}: {peak} V");
        }
    }
}

#[test]
fn clean_filters_never_self_oscillate() {
    for module_type in CLEAN {
        let out = render(module_type, 110.0, 0.001, json!(hz(1000.0)), 5.0, 48_000);
        for (port, samples) in out {
            assert!(
                peak(&samples) < 0.05,
                "{module_type} {port}: {} V",
                peak(&samples)
            );
        }
    }
}

#[test]
fn analog_filters_stay_bounded_under_audio_rate_modulation() {
    for module_type in ANALOG {
        for resonance in [0.0, 4.0, 5.0] {
            let out = render(module_type, 110.0, 5.0, cable("mod"), resonance, 24_000);
            for (port, samples) in out {
                let peak = finite_peak(&samples);
                assert!(
                    peak < 60.0,
                    "{module_type} {port} resonance {resonance}: {peak} V"
                );
            }
        }
    }
}

#[test]
fn outputs_render_the_same_whichever_others_are_read() {
    TEST_SEED_BASE.set(Some(1));
    // A Sallen-Key section that has self-oscillated runs whether or not it
    // is read, so its phase is the same when something connects to it: at
    // full resonance, where the bandpass pair alone oscillates, and with
    // the resonance swept across the threshold.
    for (module_type, resonance) in [
        ("$lpfSk", json!(5.0)),
        ("$bpfSk", json!(5.0)),
        ("$bpfSk", json!(4.49)),
        ("$lpfSk", block_common::cable("lfo", 1)),
        ("$bpfSk", block_common::cable("lfo", 1)),
    ] {
        check_connections_agree(
            module_type,
            json!({
                "input": poly_cable("src", LANES),
                freq_param(module_type): 2.0,
                "resonance": resonance,
            }),
            &["12db", "24db"],
            1e-3,
            64,
        );
    }
    for module_type in all().chain(WASP) {
        let ports: Vec<String> = schema()
            .into_iter()
            .find(|s| s.name == module_type)
            .unwrap()
            .outputs
            .into_iter()
            .map(|output| output.name)
            .collect();
        let ports: Vec<&str> = ports.iter().map(String::as_str).collect();
        let mut params = json!({ "input": poly_cable("src", LANES), "resonance": 1.0 });
        params[freq_param(module_type)] = json!(4.0);
        let variants = if CLEAN.contains(&module_type) {
            vec![params]
        } else {
            [true, false]
                .into_iter()
                .map(|high_res| {
                    let mut params = params.clone();
                    params["highRes"] = json!(high_res);
                    params
                })
                .collect()
        };
        for params in variants {
            check_connections_agree(module_type, params, &ports, 2e-2, 192);
        }
    }
}
