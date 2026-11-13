//! `$amp` through every block-input resolution path: rows read in place,
//! constants and mono cables broadcast, inputs copied out slot by slot, and
//! the per-sample `update` path, pinned to its formula.

mod block_common;

use block_common::*;
use serde_json::{Value, json};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn expected(input: &[f32], factor: &[f32]) -> Vec<f32> {
    input
        .iter()
        .zip(factor)
        .map(|(x, factor)| x * (factor / 5.0))
        .collect()
}

/// Every sample of an `$amp` with `params`; see [`check_module`].
fn check(params: Value, reference: impl Fn(&[f32], &[f32]) -> Vec<f32>) {
    check_module("$amp", params, reference);
}

#[test]
fn poly_cable_with_constant_factor() {
    check(
        json!({ "input": poly_cable("src", LANES), "factor": 4.0 }),
        |src, _| expected(src, &constant(4.0)),
    );
}

#[test]
fn every_input_a_cable() {
    check(
        json!({ "input": poly_cable("src", LANES), "factor": poly_cable("lfo", LANES) }),
        expected,
    );
}

#[test]
fn constants_cycle_across_wider_lanes() {
    // Two factors cycle over four lanes.
    check(
        json!({ "input": poly_cable("src", LANES), "factor": [1.0, 3.0] }),
        |src, _| {
            let factor: Vec<f32> = (0..src.len()).map(|k| [1.0, 3.0][k % LANES % 2]).collect();
            expected(src, &factor)
        },
    );
}

#[test]
fn mono_cable_broadcast_across_poly_lanes() {
    // The four-wide factor makes the module four lanes wide; the input is
    // lane 2 of `src` read on every lane.
    check(
        json!({ "input": cable("src", 2), "factor": [0.0, 1.0, 2.0, 3.0] }),
        |src, _| {
            let input: Vec<f32> = (0..src.len()).map(|k| src[k / LANES * LANES + 2]).collect();
            let factor: Vec<f32> = (0..src.len()).map(|k| (k % LANES) as f32).collect();
            expected(&input, &factor)
        },
    );
}

#[test]
fn mixed_cable_and_constant_lanes() {
    check(
        json!({
            "input": poly_cable("src", LANES),
            "factor": [cable("lfo", 0), 2.0, cable("lfo", 2), 7.0],
        }),
        |src, lfo| {
            let factor: Vec<f32> = (0..src.len())
                .map(|k| match k % LANES {
                    1 => 2.0,
                    3 => 7.0,
                    _ => lfo[k],
                })
                .collect();
            expected(src, &factor)
        },
    );
}

#[test]
fn permuted_cable_lanes() {
    let reversed = Value::Array((0..LANES).rev().map(|lane| cable("src", lane)).collect());
    check(json!({ "input": reversed, "factor": 2.5 }), |src, _| {
        let input: Vec<f32> = (0..src.len())
            .map(|k| src[k / LANES * LANES + (LANES - 1 - k % LANES)])
            .collect();
        expected(&input, &constant(2.5))
    });
}

#[test]
fn non_finite_results_are_silenced() {
    check(
        json!({ "input": poly_cable("src", LANES), "factor": f32::MAX }),
        |src, _| {
            expected(src, &constant(f32::MAX))
                .into_iter()
                .map(|v| if v.is_finite() { v } else { 0.0 })
                .collect()
        },
    );
}

#[test]
fn mono_cable_modulates_poly_input() {
    // `.amp(envelope)`: a poly input scaled by one lane of another module.
    check(
        json!({ "input": poly_cable("src", LANES), "factor": cable("lfo", 1) }),
        |src, lfo| {
            let factor: Vec<f32> = (0..src.len()).map(|k| lfo[k / LANES * LANES + 1]).collect();
            expected(src, &factor)
        },
    );
}

#[test]
fn cable_lanes_starting_past_zero() {
    // Lanes 1..4 of `src` make a three-wide module; reading its lane 3
    // wraps to lane 0, which is `src` lane 1.
    let shifted = Value::Array((1..LANES).map(|lane| cable("src", lane)).collect());
    check(json!({ "input": shifted, "factor": 3.0 }), |src, _| {
        let input: Vec<f32> = (0..src.len())
            .map(|k| src[k / LANES * LANES + 1 + (k % LANES) % (LANES - 1)])
            .collect();
        expected(&input, &constant(3.0))
    });
}

#[test]
fn paths_agree() {
    check_paths_agree(
        "$amp",
        json!({ "input": poly_cable("src", LANES), "factor": cable("lfo", 1) }),
    );
}

#[test]
fn swap_agrees_through_a_factor_glide() {
    let input = poly_cable("src", LANES);
    check_swap_agrees(
        "$amp",
        json!({ "input": input.clone(), "factor": 1.0 }),
        json!({ "input": input, "factor": [4.0, 8.0] }),
    );
}

#[test]
fn allocation_free() {
    for params in [
        json!({ "input": poly_cable("src", LANES), "factor": 4.0 }),
        json!({ "input": poly_cable("src", LANES), "factor": cable("src", 2) }),
        json!({ "input": cable("src", 1), "factor": [cable("src", 0), 2.0, 3.0, 4.0] }),
    ] {
        assert_eq!(allocations_while_rendering("$amp", params), 0);
    }
}

#[test]
fn bounds_scale_with_the_factor() {
    let input = poly_cable("src", LANES);
    let cases = [
        (json!(2.5), Bounds::new(-2.5, 2.5)),
        // A negative factor swaps the swing, as it inverts the signal.
        (json!(-5.0), Bounds::new(5.0, -5.0)),
        (poly_cable("lfo", LANES), Bounds::new(-5.0, 5.0)),
    ];
    for (factor, expected) in cases {
        let bounds = check_bounds_agree("$amp", json!({ "input": input, "factor": factor }));
        assert!(
            bounds.iter().all(|&b| b == expected),
            "{factor}: {:?}",
            bounds[0]
        );
    }
}
