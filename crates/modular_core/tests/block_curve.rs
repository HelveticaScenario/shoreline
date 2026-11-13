//! `$curve` renders identically on the block and per-sample paths, with its
//! `exp` read as a cable, a constant, or per-lane constants.

mod block_common;

use block_common::*;
use serde_json::{Value, json};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn cases() -> [Value; 3] {
    [
        json!({ "input": poly_cable("src", LANES), "exp": cable("lfo", 1) }),
        json!({ "input": poly_cable("src", LANES), "exp": 1.5 }),
        json!({ "input": cable("src", 2), "exp": [0.5, 1.0, 2.0, 3.0] }),
    ]
}

#[test]
fn paths_agree() {
    for params in cases() {
        check_paths_agree("$curve", params);
    }
}

#[test]
fn allocation_free() {
    for params in cases() {
        assert_eq!(allocations_while_rendering("$curve", params), 0);
    }
}

#[test]
fn bounds_bend_with_the_signal() {
    let bounds = check_bounds_agree(
        "$curve",
        json!({ "input": poly_cable("src", LANES), "exp": 2.0 }),
    );
    assert!(bounds.iter().all(|&b| b == Bounds::new(-5.0, 5.0)));
    check_bounds_agree(
        "$curve",
        json!({ "input": poly_cable("src", LANES), "exp": poly_cable("lfo", LANES) }),
    );
}
