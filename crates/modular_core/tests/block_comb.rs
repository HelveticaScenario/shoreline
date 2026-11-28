//! `$comb` renders identically on the block and per-sample paths: both
//! feedback signs, damping, pitches swept by an LFO, mono and poly inputs,
//! and glides across a patch swap.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn comb_paths_agree() {
    check_paths_agree("$comb", json!({ "input": cable("src", 0), "freq": 0.0 }));
    check_paths_agree(
        "$comb",
        json!({
            "input": poly_cable("src", LANES),
            "freq": poly_cable("lfo", LANES),
            "feedback": [5.0, -5.0, 2.5, -1.0],
            "damp": 2.0,
        }),
    );
    check_paths_agree(
        "$comb",
        json!({
            "input": cable("src", 1),
            "freq": [cable("lfo", 0), 3.0],
            "feedback": cable("lfo", 1),
            "damp": cable("lfo", 2),
        }),
    );
}

#[test]
fn comb_swaps_agree() {
    check_swap_agrees(
        "$comb",
        json!({ "input": cable("src", 0), "freq": 0.0, "feedback": 4.0 }),
        json!({ "input": cable("src", 0), "freq": 1.0, "feedback": -4.0, "damp": 3.0 }),
    );
}

#[test]
fn allocation_free() {
    for params in [
        json!({
            "input": poly_cable("src", LANES),
            "freq": cable("lfo", 0),
            "feedback": cable("lfo", 1),
            "damp": cable("lfo", 2),
        }),
        json!({
            "input": poly_cable("src", LANES),
            "freq": [-3.71, 0.0, 5.0, 9.0],
            "feedback": -5.0,
        }),
    ] {
        assert_eq!(allocations_while_rendering("$comb", params), 0);
    }
}
