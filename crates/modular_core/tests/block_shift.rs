//! `$shift` through the block-input resolution paths and the per-sample
//! `update` path, pinned to its formula.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn expected(input: &[f32], offset: &[f32]) -> Vec<f32> {
    input
        .iter()
        .zip(offset)
        .map(|(x, offset)| x + offset)
        .collect()
}

#[test]
fn poly_cable_with_constant_offset() {
    check_module(
        "$shift",
        json!({ "input": poly_cable("src", CHANNELS), "offset": 0.5 }),
        |src, _| expected(src, &constant(0.5)),
    );
}

#[test]
fn every_input_a_cable() {
    check_module(
        "$shift",
        json!({ "input": poly_cable("src", CHANNELS), "offset": poly_cable("lfo", CHANNELS) }),
        expected,
    );
}

#[test]
fn constants_cycle_across_wider_channels() {
    check_module(
        "$shift",
        json!({ "input": cable("src", 2), "offset": [0.0, 1.0, 2.0, 3.0] }),
        |src, _| {
            let input: Vec<f32> = (0..src.len())
                .map(|k| src[k / CHANNELS * CHANNELS + 2])
                .collect();
            let offset: Vec<f32> = (0..src.len()).map(|k| (k % CHANNELS) as f32).collect();
            expected(&input, &offset)
        },
    );
}

#[test]
fn paths_agree() {
    check_paths_agree(
        "$shift",
        json!({ "input": poly_cable("src", CHANNELS), "offset": cable("lfo", 1) }),
    );
}

#[test]
fn allocation_free() {
    for params in [
        json!({ "input": poly_cable("src", CHANNELS), "offset": [0.1, 0.2, 0.3, 0.4] }),
        json!({ "input": cable("src", 1), "offset": [cable("src", 0), 2.0, 3.0, 4.0] }),
    ] {
        assert_eq!(allocations_while_rendering("$shift", params), 0);
    }
}
