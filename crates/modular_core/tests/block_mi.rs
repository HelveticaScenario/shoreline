//! The `$mi.*` modules in a patch: block and per-sample paths agree (held and
//! modulated controls, audio-rate and through-zero FM, mono and poly inputs),
//! rendering allocates nothing, and module-level behaviour holds.

mod block_common;
mod dx7_common;

use block_common::*;
use dx7_common::plaits_voice;
use modular_core::types::{ProcessingMode, SameIds};
use serde_json::{Value, json};

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
            "ratio": poly_cable("lfo", LANES),
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
            "shape": poly_cable("lfo", LANES),
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
    check_paths_agree(
        "$mi.zOsc",
        json!({ "freq": [-1.0, 0.0, 1.5, 3.0], "phaseOffset": 0.25, "fm": cable("src", 1) }),
    );
    check_paths_agree(
        "$mi.zOsc",
        json!({ "freq": 1.0, "shape": 2.0, "phaseOffset": cable("lfo", 0) }),
    );
    check_paths_agree(
        "$mi.zOsc",
        json!({ "freq": cable("lfo", 2), "phaseOffset": poly_cable("lfo", LANES) }),
    );
}

#[test]
fn allocation_free() {
    for module in [
        "$mi.additive",
        "$mi.chiptune",
        "$mi.chords",
        "$mi.divideDown",
        "$mi.formant",
        "$mi.hiHat",
        "$mi.kick",
        "$mi.modal",
        "$mi.noise",
        "$mi.particles",
        "$mi.phaseDistortion",
        "$mi.sixOp",
        "$mi.snare",
        "$mi.speak",
        "$mi.string",
        "$mi.stringMachine",
        "$mi.swarm",
        "$mi.synthKick",
        "$mi.synthSnare",
        "$mi.twoOp",
        "$mi.va",
        "$mi.vcf",
        "$mi.formantVowel",
        "$mi.samVowel",
        "$mi.lpcVowel",
        "$mi.waveTerrain",
        "$mi.waveshaper",
        "$mi.zOsc",
    ] {
        let mut params = json!({ "freq": poly_cable("lfo", LANES), "fm": cable("src", 0) });
        if module == "$mi.sixOp" {
            params["voice"] = plaits_voice(1, 0);
        }
        assert_eq!(allocations_while_rendering(module, params), 0, "{module}");
    }
}

/// Played notes allocate nothing.
#[test]
fn six_op_allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$mi.sixOp",
            json!({ "freq": poly_cable("lfo", LANES), "gate": poly_cable("src", LANES),
                    "voice": plaits_voice(1, 0), "envelope": cable("lfo", 3) })
        ),
        0
    );
}

/// Arpeggios, with the chord and pattern moving under them, allocate nothing.
#[test]
fn chiptune_allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$mi.chiptune",
            json!({ "freq": poly_cable("lfo", LANES), "trigger": poly_cable("src", LANES),
                    "chord": cable("lfo", 2), "pattern": cable("lfo", 3), "bassDecay": true })
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
                json!({ "freq": poly_cable("lfo", LANES), "trigger": poly_cable("src", LANES),
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
            json!({ "freq": poly_cable("lfo", LANES), "trigger": poly_cable("src", LANES),
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
        (
            "$mi.hiHat",
            &["tone", "decay", "noisiness", "accent", "mix"],
        ),
    ] {
        let mut params =
            json!({ "freq": poly_cable("lfo", LANES), "trigger": poly_cable("src", LANES) });
        for (i, &knob) in knobs.iter().enumerate() {
            params[knob] = cable("lfo", i % LANES);
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
            for lane in 0..LANES {
                out.push(patch.sampleables["dut"].get_value_at(port, lane, index));
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
    let noisy_hat = json!({ "freq": 0.0, "noisiness": 5.0 });
    for (module, params, port) in [
        (
            "$mi.synthKick",
            json!({ "freq": 0.0, "dirt": 5.0 }),
            "output",
        ),
        ("$mi.snare", json!({ "freq": 0.0, "snappy": 5.0 }), "output"),
        (
            "$mi.synthSnare",
            json!({ "freq": 0.0, "snappy": 5.0 }),
            "output",
        ),
        ("$mi.hiHat", noisy_hat.clone(), "square"),
        ("$mi.hiHat", noisy_hat, "ring"),
    ] {
        let a = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            module,
            params.clone(),
        );
        let b = build(ProcessingMode::Block, ProcessingMode::Block, module, params);
        let a = render_ports(&a, port, 64);
        let b = render_ports(&b, port, 64);
        assert!(
            a.iter().zip(&b).any(|(x, y)| x != y),
            "{module} {port}: identical noise"
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
            "trigger": poly_cable("lfo", LANES),
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

/// Loud, fully accented strikes and drones stay within the ranges the
/// outputs declare.
#[test]
fn resonator_outputs_stay_in_range() {
    for module in ["$mi.modal", "$mi.string"] {
        for params in [
            json!({ "freq": cable("lfo", 0), "trigger": poly_cable("src", LANES),
                    "structure": 1.4, "brightness": 5.0, "decay": 5.0, "accent": 5.0 }),
            json!({ "freq": -2.0, "brightness": 4.0, "decay": 5.0, "accent": 5.0,
                    "input": poly_cable("src", LANES) }),
        ] {
            let patch = build(ProcessingMode::Block, ProcessingMode::Block, module, params);
            for (port, range) in [(default_port(module).as_str(), 10.0), ("exciter", 5.0)] {
                let peak = render_ports(&patch, port, 200)
                    .iter()
                    .fold(0.0f32, |m, v| m.max(v.abs()));
                assert!(peak <= range, "{module} {port} peak {peak}");
            }
        }
    }
}

/// The modules with Plaits' main and aux outputs, and the two outputs' names.
const MAIN_AND_AUX: [(&str, &str, &str); 17] = [
    ("$mi.hiHat", "square", "ring"),
    ("$mi.va", "dual", "sync"),
    ("$mi.vcf", "lowpass", "highpass"),
    ("$mi.noise", "filtered", "bands"),
    ("$mi.chords", "chord", "root"),
    ("$mi.chiptune", "chord", "bass"),
    ("$mi.twoOp", "fm", "subOctave"),
    ("$mi.waveshaper", "shaped", "overtones"),
    ("$mi.waveTerrain", "height", "pm"),
    ("$mi.swarm", "saw", "sine"),
    ("$mi.modal", "body", "exciter"),
    ("$mi.string", "strings", "exciter"),
    ("$mi.particles", "filtered", "raw"),
    ("$mi.speak", "output", "excitation"),
    ("$mi.formantVowel", "output", "excitation"),
    ("$mi.samVowel", "output", "excitation"),
    ("$mi.lpcVowel", "output", "excitation"),
];

/// Each module with Plaits' main and aux outputs reads by default as `blend`:
/// the two crossfaded by `mix`, exactly the main at 0 V and the aux at 5 V.
#[test]
fn blend_crossfades_main_into_aux() {
    let pairs = MAIN_AND_AUX;
    let freq = json!([-1.0, 0.0, 0.5, 1.0]);
    for (module, main, aux) in pairs {
        assert_eq!(default_port(module), "blend", "{module}");
        for mix in [json!(0.0), json!(5.0), json!(2.5), poly_cable("lfo", LANES)] {
            let mut params = json!({ "freq": freq, "mix": mix });
            if matches!(module, "$mi.modal" | "$mi.string") {
                params["brightness"] = json!(5.0);
            }
            if module == "$mi.particles" {
                params["density"] = json!(5.0);
            }
            let patch = build(
                ProcessingMode::Block,
                ProcessingMode::Sample,
                module,
                params,
            );
            let (mut heard_main, mut heard_aux) = (false, false);
            for _ in 0..40 {
                render(&patch, &[BLOCK_SIZE]);
                for index in 0..BLOCK_SIZE {
                    for lane in 0..LANES {
                        let read = |id: &str, port: &str| {
                            patch.sampleables[id].get_value_at(port, lane, index)
                        };
                        let (m, a) = (read("dut", main), read("dut", aux));
                        let volts = match mix.as_f64() {
                            Some(volts) => volts as f32,
                            None => read("lfo", "output"),
                        };
                        let t = volts.clamp(0.0, 5.0) * 0.2;
                        assert_eq!(
                            read("dut", "blend"),
                            m * (1.0 - t) + a * t,
                            "{module} mix {mix} lane {lane}"
                        );
                        heard_main |= m != 0.0;
                        heard_aux |= a != 0.0;
                    }
                }
            }
            assert!(heard_main && heard_aux, "{module}: silent output");
        }
    }
}

/// A patch update that turns `mix` from 0 to 5 glides `blend` from the main
/// output into the aux on the lanes it keeps; a lane it adds has no earlier
/// sound and starts on the aux.
#[test]
fn blend_glides_when_mix_changes() {
    const KEPT: usize = 2;
    for (module, main, aux) in MAIN_AND_AUX {
        let params = |freq: Value, mix: f32| {
            let mut params = json!({ "freq": freq, "mix": mix });
            if matches!(module, "$mi.modal" | "$mi.string") {
                params["brightness"] = json!(5.0);
            }
            if module == "$mi.particles" {
                params["density"] = json!(5.0);
            }
            params
        };
        let old = build(
            ProcessingMode::Block,
            ProcessingMode::Sample,
            module,
            params(json!([0.0, 0.5]), 0.0),
        );
        for _ in 0..8 {
            render(&old, &[BLOCK_SIZE]);
        }
        let new = build(
            ProcessingMode::Block,
            ProcessingMode::Sample,
            module,
            params(json!([0.0, 0.5, -1.0, 1.0]), 5.0),
        );
        for (id, sampleable) in &new.sampleables {
            sampleable.transfer_state_from(old.sampleables[id].as_ref(), &SameIds);
        }
        new.connect_all();
        let read = |port: &str, lane: usize, index: usize| {
            new.sampleables["dut"].get_value_at(port, lane, index)
        };
        for sampleable in new.sampleables.values() {
            sampleable.start_block();
        }
        for id in ["dut", "src", "lfo"] {
            new.sampleables[id].ensure_processed_to(BLOCK_SIZE);
        }
        let last = BLOCK_SIZE - 1;
        for lane in KEPT..LANES {
            assert_eq!(
                read("blend", lane, 0),
                read(aux, lane, 0),
                "{module} lane {lane}"
            );
        }
        for lane in 0..KEPT {
            assert_eq!(
                read("blend", lane, 0),
                read(main, lane, 0),
                "{module} lane {lane}"
            );
            let (m, a) = (read(main, lane, last), read(aux, lane, last));
            if (a - m).abs() > 1e-3 {
                let t = (read("blend", lane, last) - m) / (a - m);
                assert!(
                    0.01 < t && t < 0.99,
                    "{module} lane {lane}: {t} of the way to aux"
                );
            }
        }
        for _ in 0..40 {
            render(&new, &[BLOCK_SIZE]);
        }
        for lane in 0..LANES {
            assert_eq!(
                read("blend", lane, 0),
                read(aux, lane, 0),
                "{module} lane {lane}"
            );
        }
    }
}

/// A side added by a patch update draws noise of its own for each hat.
#[test]
fn hi_hat_sides_added_by_a_patch_update_get_their_own_noise() {
    use modular_core::dsp::{get_constructors, get_params_deserializers};
    use modular_core::params::DeserializedParams;
    modular_core::dsp::utils::rng::TEST_SEED_BASE.set(Some(1));
    let make = |channels: usize| {
        let cached =
            get_params_deserializers()["$mi.hiHat"](json!({ "freq": 0.0, "noisiness": 5.0 }))
                .unwrap();
        get_constructors()["$mi.hiHat"](
            "dut",
            48000.0,
            DeserializedParams {
                params: cached.params,
                lane_count: cached.lane_count * channels,
                f64_inputs: false,
                channels,
            },
            1,
            ProcessingMode::Block,
        )
        .unwrap()
    };
    let old = make(1);
    old.on_patch_update();
    for _ in 0..480 {
        old.start_block();
        old.ensure_processed();
    }
    let new = make(2);
    new.transfer_state_from(old.as_ref(), &SameIds);
    new.on_patch_update();
    for port in ["square", "ring"] {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for _ in 0..4800 {
            new.start_block();
            new.ensure_processed();
            a.push(new.get_value_at(port, 0, 0));
            b.push(new.get_value_at(port, 1, 0));
        }
        assert!(a.iter().any(|v| *v != 0.0), "{port} silent");
        assert!(
            a.iter().zip(&b).any(|(x, y)| x != y),
            "{port}: sides share noise"
        );
    }
}
