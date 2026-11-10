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
        "$mi.formant",
        "$mi.modal",
        "$mi.phaseDistortion",
        "$mi.twoOp",
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

/// Two `$mi.modal` drones with the same settings draw their own grains.
#[test]
fn modal_instances_draw_their_own_grains() {
    let params = json!({ "freq": 0.0, "brightness": 5.0 });
    let a = build(
        ProcessingMode::Block,
        ProcessingMode::Block,
        "$mi.modal",
        params.clone(),
    );
    let b = build(
        ProcessingMode::Block,
        ProcessingMode::Block,
        "$mi.modal",
        params,
    );
    let a = render_ports(&a, "exciter", 4);
    let b = render_ports(&b, "exciter", 4);
    assert!(
        a.iter().zip(&b).any(|(x, y)| x != y),
        "identical grain streams"
    );
}

/// Loud strikes and drones stay within the ±5 V the outputs declare.
#[test]
fn modal_outputs_stay_in_range() {
    for params in [
        json!({ "freq": cable("lfo", 0), "trigger": poly_cable("src", CHANNELS),
                "structure": 1.4, "brightness": 5.0, "decay": 5.0, "accent": 5.0 }),
        json!({ "freq": -2.0, "brightness": 4.0, "decay": 5.0, "accent": 5.0,
                "input": poly_cable("src", CHANNELS) }),
    ] {
        let patch = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            "$mi.modal",
            params,
        );
        for port in ["output", "exciter"] {
            let peak = render_ports(&patch, port, 200)
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak <= 5.0, "{port} peak {peak}");
        }
    }
}
