//! `$pulse` renders identically on the block and per-sample paths: held and
//! modulated pitch and width, PWM, FM, phase offset and hard sync.

mod block_common;

use block_common::*;
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
