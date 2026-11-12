//! `$erratic` through every block-input resolution path. Its shift is random,
//! so the exact checks use a zero `amount`, where the output is the input
//! clamped to `[min, max]`; the in-module tests pin the shift itself, comparing
//! `process_block` against `update` from one seeded state.

mod block_common;

use block_common::*;
use modular_core::types::ProcessingMode;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn zero_amount_passes_cable_input_through() {
    check_module(
        "$erratic",
        json!({ "input": poly_cable("src", CHANNELS), "amount": 0.0, "speed": cable("lfo", 1) }),
        |src, _| src.to_vec(),
    );
}

#[test]
fn clamps_to_cable_range() {
    check_module(
        "$erratic",
        json!({
            "input": poly_cable("src", CHANNELS),
            "amount": 0.0,
            "trigger": poly_cable("lfo", CHANNELS),
            "min": poly_cable("lfo", CHANNELS),
            "max": [5.0, cable("lfo", 0), 5.0, 5.0],
            "mode": "clamp",
        }),
        |src, lfo| {
            src.iter()
                .zip(lfo)
                .enumerate()
                .map(|(k, (&x, &lo))| {
                    let hi = if k % CHANNELS == 1 {
                        lfo[k / CHANNELS * CHANNELS]
                    } else {
                        5.0
                    };
                    x.clamp(lo.min(hi), lo.max(hi))
                })
                .collect()
        },
    );
}

#[test]
fn allocation_free() {
    for params in [
        json!({ "input": poly_cable("src", CHANNELS), "amount": 2.0 }),
        json!({ "input": poly_cable("src", CHANNELS), "amount": cable("lfo", 0), "speed": poly_cable("lfo", CHANNELS), "trigger": poly_cable("src", CHANNELS) }),
        json!({ "input": cable("src", 1), "amount": [1.0, 2.0, 3.0, 4.0], "bias": [cable("lfo", 2), 1.0], "min": -1.0, "max": 1.0, "mode": "clamp" }),
        json!({ "input": poly_cable("src", CHANNELS), "amount": 5.0, "speed": 5.0, "trigger": poly_cable("src", CHANNELS), "mode": "blend", "sparse": true }),
    ] {
        assert_eq!(allocations_while_rendering("$erratic", params), 0);
    }
}

#[test]
fn bounds_are_the_range_in_its_order_unless_shifting() {
    let input = poly_cable("src", CHANNELS);
    for mode in ["blend", "clamp"] {
        let bounds = check_bounds_agree(
            "$erratic",
            json!({ "input": input, "amount": 3.0, "min": 3.0, "max": -1.0, "mode": mode }),
        );
        assert!(
            bounds.iter().all(|&b| b == Bounds::new(3.0, -1.0)),
            "{mode}"
        );
    }
    let patch = build(
        ProcessingMode::Block,
        ProcessingMode::Block,
        "$erratic",
        json!({ "input": input, "amount": 3.0, "mode": "shift" }),
    );
    assert!(
        render_bounds(&patch, &[BLOCK_SIZE])
            .iter()
            .all(|b| !b.is_bounded())
    );
}

#[test]
fn an_unset_mode_blends_within_a_ranged_input() {
    // The saw swings over ±5, so an unset mode blends within it.
    let bounds = check_bounds_agree(
        "$erratic",
        json!({ "input": poly_cable("src", CHANNELS), "amount": 3.0 }),
    );
    assert!(bounds.iter().all(|&b| b == Bounds::new(-5.0, 5.0)));
    // A fixed voltage has no range, so an unset mode shifts.
    let patch = build(
        ProcessingMode::Block,
        ProcessingMode::Block,
        "$erratic",
        json!({ "input": 1.0, "amount": 3.0 }),
    );
    assert!(
        render_bounds(&patch, &[BLOCK_SIZE])
            .iter()
            .all(|b| !b.is_bounded())
    );
}
