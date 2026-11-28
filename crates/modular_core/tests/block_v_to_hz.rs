//! `$vToHz` renders identically on the block and per-sample paths, pinned to
//! C4 · 2^v.

mod block_common;

use block_common::*;
use serde_json::{Value, json};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

const C4_HZ: f32 = 261.625_58;

fn cases() -> [Value; 2] {
    [
        json!({ "input": poly_cable("src", LANES) }),
        json!({ "input": cable("src", 2) }),
    ]
}

#[test]
fn converts_voct_to_hz() {
    check_module(
        "$vToHz",
        json!({ "input": poly_cable("src", LANES) }),
        |src, _| src.iter().map(|&v| C4_HZ * v.exp2()).collect(),
    );
}

#[test]
fn paths_agree() {
    for params in cases() {
        check_paths_agree("$vToHz", params);
    }
}

#[test]
fn allocation_free() {
    for params in cases() {
        assert_eq!(allocations_while_rendering("$vToHz", params), 0);
    }
}
