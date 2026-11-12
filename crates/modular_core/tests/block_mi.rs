//! The `$mi.*` modules in a patch: block and per-sample paths agree (held and
//! modulated controls, audio-rate and through-zero FM, mono and poly inputs),
//! rendering allocates nothing, and module-level behaviour holds.

mod block_common;

use block_common::*;
use modular_core::types::ProcessingMode;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn formant_paths_agree() {
    check_paths_agree("$mi.formant", json!({ "freq": 0.0 }));
    check_paths_agree(
        "$mi.formant",
        json!({
            "freq": [-1.0, 0.0, 1.5, 3.0],
            "formant": cable("lfo", 0),
            "ratio": poly_cable("lfo", CHANNELS),
            "width": 4.0,
            "fm": cable("src", 1),
        }),
    );
    check_paths_agree(
        "$mi.formant",
        json!({
            "freq": cable("lfo", 2),
            "ratio": 1.0,
            "fm": poly_cable("src", 3),
            "fmMode": "exp",
        }),
    );
}

#[test]
fn z_osc_paths_agree() {
    check_paths_agree("$mi.zOsc", json!({ "freq": 0.0 }));
    check_paths_agree(
        "$mi.zOsc",
        json!({
            "freq": [-1.0, 0.0, 1.5, 3.0],
            "cutoff": cable("lfo", 0),
            "shape": poly_cable("lfo", CHANNELS),
            "mode": 1.0,
            "fm": cable("src", 1),
        }),
    );
    check_paths_agree(
        "$mi.zOsc",
        json!({
            "freq": cable("lfo", 2),
            "shape": 4.5,
            "mode": 4.0,
            "fm": poly_cable("src", 3),
            "fmMode": "lin",
        }),
    );
}

#[test]
fn allocation_free() {
    for module in [
        "$mi.additive",
        "$mi.divideDown",
        "$mi.formant",
        "$mi.hiHat",
        "$mi.kick",
        "$mi.modal",
        "$mi.particles",
        "$mi.phaseDistortion",
        "$mi.ringHat",
        "$mi.sixOp",
        "$mi.snare",
        "$mi.speak",
        "$mi.string",
        "$mi.swarm",
        "$mi.synthKick",
        "$mi.synthSnare",
        "$mi.twoOp",
        "$mi.formantVowel",
        "$mi.samVowel",
        "$mi.lpcVowel",
        "$mi.waveTerrain",
        "$mi.waveshaper",
        "$mi.zOsc",
    ] {
        assert_eq!(
            allocations_while_rendering(
                module,
                json!({ "freq": poly_cable("lfo", CHANNELS), "fm": cable("src", 0) })
            ),
            0,
            "{module}"
        );
    }
}

/// Played notes, with the patch and bank switching under them, allocate
/// nothing.
#[test]
fn six_op_allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$mi.sixOp",
            json!({ "freq": poly_cable("lfo", CHANNELS), "gate": poly_cable("src", CHANNELS),
                    "bank": cable("lfo", 1), "patch": cable("lfo", 2), "envelope": cable("lfo", 3) })
        ),
        0
    );
}

/// Syllables on every vowel synth, with the vowel and formant shift moving,
/// allocate nothing.
#[test]
fn vowel_allocation_free() {
    for module in ["$mi.formantVowel", "$mi.samVowel", "$mi.lpcVowel"] {
        assert_eq!(
            allocations_while_rendering(
                module,
                json!({ "freq": poly_cable("lfo", CHANNELS), "trigger": poly_cable("src", CHANNELS),
                        "vowel": cable("lfo", 2), "formantShift": cable("lfo", 3) })
            ),
            0,
            "{module}"
        );
    }
}

/// Triggered words, with the bank switching and every knob moving under
/// them, allocate nothing.
#[test]
fn speak_allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$mi.speak",
            json!({ "freq": poly_cable("lfo", CHANNELS), "trigger": poly_cable("src", CHANNELS),
                    "bank": cable("lfo", 0), "word": cable("lfo", 1), "formantShift": cable("lfo", 2),
                    "prosody": cable("lfo", 3), "speed": cable("lfo", 1), "accent": cable("lfo", 2) })
        ),
        0
    );
}

/// Strikes, with every knob moving under them, allocate nothing.
#[test]
fn drum_allocation_free() {
    for (module, knobs) in [
        (
            "$mi.kick",
            &["tone", "decay", "attackFm", "selfFm", "drive", "accent"][..],
        ),
        (
            "$mi.synthKick",
            &["tone", "decay", "dirt", "fmEnvelope", "fmDecay", "accent"],
        ),
        ("$mi.snare", &["tone", "decay", "snappy", "accent"]),
        ("$mi.synthSnare", &["attackFm", "decay", "snappy", "accent"]),
        ("$mi.hiHat", &["tone", "decay", "noisiness", "accent"]),
        ("$mi.ringHat", &["tone", "decay", "noisiness", "accent"]),
    ] {
        let mut params =
            json!({ "freq": poly_cable("lfo", CHANNELS), "trigger": poly_cable("src", CHANNELS) });
        for (i, &knob) in knobs.iter().enumerate() {
            params[knob] = cable("lfo", i % CHANNELS);
        }
        assert_eq!(allocations_while_rendering(module, params), 0, "{module}");
    }
}

#[test]
fn chorus_allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$mi.chorus",
            json!({ "input": poly_cable("src", 2), "amount": cable("lfo", 0), "depth": cable("lfo", 1) })
        ),
        0
    );
}

fn render_ports(patch: &modular_core::Patch, port: &str, calls: usize) -> Vec<f32> {
    let mut out = Vec::new();
    for _ in 0..calls {
        render(patch, &[BLOCK_SIZE]);
        for index in 0..BLOCK_SIZE {
            for ch in 0..CHANNELS {
                out.push(patch.sampleables["dut"].get_value_at(port, ch, index));
            }
        }
    }
    out
}

/// Two drones with the same settings draw their own grains.
#[test]
fn resonator_instances_draw_their_own_grains() {
    for module in ["$mi.modal", "$mi.string"] {
        let params = json!({ "freq": 0.0, "brightness": 5.0 });
        let a = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            module,
            params.clone(),
        );
        let b = build(ProcessingMode::Block, ProcessingMode::Block, module, params);
        let a = render_ports(&a, "exciter", 4);
        let b = render_ports(&b, "exciter", 4);
        assert!(
            a.iter().zip(&b).any(|(x, y)| x != y),
            "{module}: identical grains"
        );
    }
}

/// Two noisy drones with the same settings draw their own noise.
#[test]
fn drum_instances_draw_their_own_noise() {
    for (module, params) in [
        ("$mi.synthKick", json!({ "freq": 0.0, "dirt": 5.0 })),
        ("$mi.snare", json!({ "freq": 0.0, "snappy": 5.0 })),
        ("$mi.synthSnare", json!({ "freq": 0.0, "snappy": 5.0 })),
        ("$mi.hiHat", json!({ "freq": 0.0, "noisiness": 5.0 })),
        ("$mi.ringHat", json!({ "freq": 0.0, "noisiness": 5.0 })),
    ] {
        let a = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            module,
            params.clone(),
        );
        let b = build(ProcessingMode::Block, ProcessingMode::Block, module, params);
        let a = render_ports(&a, "output", 64);
        let b = render_ports(&b, "output", 64);
        assert!(
            a.iter().zip(&b).any(|(x, y)| x != y),
            "{module}: identical noise"
        );
    }
}

/// Two instances triggered alike draw their own consonant noise. The
/// consonant a trigger plays follows the vowel, and only unvoiced ones carry
/// noise, so the vowel sweeps down from the top of its range until one does.
#[test]
fn vowel_instances_draw_their_own_noise() {
    let differ = (0..=10).rev().any(|step| {
        let params = json!({
            "freq": 0.0,
            "vowel": step as f32 * 0.5,
            "trigger": poly_cable("lfo", CHANNELS),
        });
        let a = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            "$mi.lpcVowel",
            params.clone(),
        );
        let b = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            "$mi.lpcVowel",
            params,
        );
        let a = render_ports(&a, "excitation", 64);
        let b = render_ports(&b, "excitation", 64);
        a.iter().zip(&b).any(|(x, y)| x != y)
    });
    assert!(differ, "every vowel drew identical noise");
}

/// Loud strikes and drones stay within the ±5 V the outputs declare.
#[test]
fn resonator_outputs_stay_in_range() {
    for module in ["$mi.modal", "$mi.string"] {
        for params in [
            json!({ "freq": cable("lfo", 0), "trigger": poly_cable("src", CHANNELS),
                    "structure": 1.4, "brightness": 5.0, "decay": 5.0, "accent": 5.0 }),
            json!({ "freq": -2.0, "brightness": 4.0, "decay": 5.0, "accent": 5.0,
                    "input": poly_cable("src", CHANNELS) }),
        ] {
            let patch = build(ProcessingMode::Block, ProcessingMode::Block, module, params);
            for port in [default_port(module).as_str(), "exciter"] {
                let peak = render_ports(&patch, port, 200)
                    .iter()
                    .fold(0.0f32, |m, v| m.max(v.abs()));
                assert!(peak <= 5.0, "{module} {port} peak {peak}");
            }
        }
    }
}
