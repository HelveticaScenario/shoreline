//! `$gain` through the block-input resolution paths and the per-sample
//! `update` path, pinned to its audio-taper formula.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn expected(input: &[f32], level: &[f32]) -> Vec<f32> {
    input
        .iter()
        .zip(level)
        .map(|(x, level)| {
            let g = level / 5.0;
            x * (g * g * g)
        })
        .collect()
}

#[test]
fn poly_cable_with_constant_level() {
    check_module(
        "$gain",
        json!({ "input": poly_cable("src", CHANNELS), "level": 2.5 }),
        |src, _| expected(src, &constant(2.5)),
    );
}

#[test]
fn unity_at_five() {
    check_module(
        "$gain",
        json!({ "input": poly_cable("src", CHANNELS), "level": 5.0 }),
        |src, _| src.to_vec(),
    );
}

#[test]
fn every_input_a_cable() {
    check_module(
        "$gain",
        json!({ "input": poly_cable("src", CHANNELS), "level": poly_cable("lfo", CHANNELS) }),
        expected,
    );
}

#[test]
fn paths_agree() {
    check_paths_agree(
        "$gain",
        json!({ "input": poly_cable("src", CHANNELS), "level": cable("lfo", 1) }),
    );
}

#[test]
fn allocation_free() {
    for params in [
        json!({ "input": poly_cable("src", CHANNELS), "level": 4.0 }),
        json!({ "input": cable("src", 1), "level": [cable("src", 0), 2.0, 3.0, 4.0] }),
    ] {
        assert_eq!(allocations_while_rendering("$gain", params), 0);
    }
}
