//! `$pulse` renders identically on the block and per-sample paths: held and
//! modulated pitch and width, PWM, FM, phase offset and hard sync, band-limited
//! or not.

mod block_common;

use block_common::*;
use modular_core::types::ProcessingMode;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn pulse_paths_agree() {
    check_paths_agree("$pulse", json!({ "freq": [0.0, 0.5, -1.0, 2.0] }));
    check_paths_agree(
        "$pulse",
        json!({
            "freq": poly_cable("lfo", LANES),
            "fm": cable("src", 1),
            "fmMode": "lin",
            "width": [1.0, 2.5, 4.0, 0.0],
            "pwm": poly_cable("lfo", LANES),
            "phaseOffset": 0.25,
        }),
    );
    check_paths_agree(
        "$pulse",
        json!({ "freq": 1.0, "sync": poly_cable("src", LANES), "phaseOffset": cable("lfo", 0) }),
    );
    check_paths_agree(
        "$pulse",
        json!({
            "freq": poly_cable("lfo", LANES),
            "pwm": poly_cable("lfo", LANES),
            "sync": cable("src", 1),
            "bandlimit": false,
        }),
    );
}

/// Without band-limiting every sample is fully high or fully low, through
/// width modulation and hard sync alike.
#[test]
fn unbandlimited_pulse_only_switches() {
    let outputs = |bandlimit: bool| {
        let patch = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            "$pulse",
            json!({
                "freq": [2.0, 3.0, 1.0, 4.0],
                "pwm": poly_cable("lfo", LANES),
                "sync": cable("src", 1),
                "bandlimit": bandlimit,
            }),
        );
        render(&patch, &[BLOCK_SIZE]).0
    };
    assert!(outputs(false).iter().all(|&v| v.abs() == 5.0));
    assert!(
        outputs(true).iter().any(|&v| v.abs() != 5.0),
        "band-limited edges are smoothed"
    );
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$pulse",
            json!({ "freq": poly_cable("src", LANES), "pwm": cable("src", 0), "sync": cable("src", 1) })
        ),
        0
    );
}

#[test]
fn swap_agrees_through_width_and_pwm_glides() {
    check_swap_agrees(
        "$pulse",
        json!({ "freq": [0.0, 0.5, 1.0, 1.5], "width": 1.0, "pwm": 0.0 }),
        json!({ "freq": [0.0, 0.5, 1.0, 1.5], "width": 4.0, "pwm": cable("lfo", 0) }),
    );
}
