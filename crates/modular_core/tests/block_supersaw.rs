//! `$supersaw` renders identically on the block and per-sample paths: held and
//! modulated pitch and detune, FM, hard sync, and mono and poly inputs.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn supersaw_paths_agree() {
    check_paths_agree("$supersaw", json!({ "freq": 0.0, "unison": 4 }));
    check_paths_agree(
        "$supersaw",
        json!({ "freq": [0.0, 0.5, -1.0], "unison": 7, "detune": [0.3, 1.0, 0.0] }),
    );
    check_paths_agree(
        "$supersaw",
        json!({
            "freq": poly_cable("lfo", LANES),
            "unison": 4,
            "fm": cable("src", 1),
            "fmMode": "exp",
            "detune": cable("lfo", 0),
        }),
    );
    check_paths_agree(
        "$supersaw",
        json!({ "freq": 1.0, "unison": 1, "sync": cable("src", 2), "waitForZeroCross": false }),
    );
    check_paths_agree(
        "$supersaw",
        json!({ "freq": [1.0, -1.0], "unison": 5, "sync": poly_cable("src", 2) }),
    );
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$supersaw",
            json!({ "freq": poly_cable("src", LANES), "unison": 6, "fm": cable("src", 0), "sync": cable("src", 1) })
        ),
        0
    );
}

#[test]
fn supersaw_bounds_hold_every_sample() {
    let params = json!({ "freq": [0.0, 0.7], "unison": 7, "detune": 0.0 });
    let bounds = check_bounds_agree("$supersaw", params.clone());
    // Seven saws in step swing each side past 5 V.
    assert!(bounds.iter().any(|b| b.end > 5.0), "{:?}", &bounds[..2]);
    let patch = build(
        modular_core::types::ProcessingMode::Block,
        modular_core::types::ProcessingMode::Block,
        "$supersaw",
        params,
    );
    let (samples, _, _, bounds) = render_with_bounds(&patch, &[BLOCK_SIZE]);
    for (v, b) in samples.iter().zip(&bounds) {
        assert!(b.start <= *v && *v <= b.end, "{v} outside {b:?}");
    }
}
