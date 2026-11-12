//! `$signal` forwards its value identically on the block and per-sample paths.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn signal_forwards_cables_and_constants() {
    check_module(
        "$signal",
        json!({ "value": poly_cable("src", CHANNELS) }),
        |src, lfo| elementwise(src, lfo, |x, _| x),
    );
    check_module(
        "$signal",
        json!({ "value": [0.5, -1.0, 2.0, 3.5] }),
        |src, _| {
            (0..src.len())
                .map(|k| [0.5, -1.0, 2.0, 3.5][k % CHANNELS])
                .collect()
        },
    );
}

#[test]
fn paths_agree() {
    check_paths_agree("$signal", json!({ "value": poly_cable("src", CHANNELS) }));
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering("$signal", json!({ "value": poly_cable("src", CHANNELS) })),
        0
    );
}

#[test]
fn bounds_pass_through() {
    let bounds = check_bounds_agree("$signal", json!({ "value": poly_cable("src", CHANNELS) }));
    assert!(bounds.iter().all(|&b| b == Bounds::new(-5.0, 5.0)));
    let bounds = check_bounds_agree("$signal", json!({ "value": 3.0 }));
    assert!(bounds.iter().all(|&b| b == Bounds::point(3.0)));
}
