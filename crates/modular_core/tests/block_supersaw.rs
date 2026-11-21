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
        json!({ "freq": 1.0, "unison": 1, "sync": 1.3, "waitForZeroCross": false }),
    );
    check_paths_agree(
        "$supersaw",
        json!({ "freq": [1.0, -1.0], "unison": 5, "sync": cable("lfo", 0) }),
    );
    check_paths_agree(
        "$supersaw",
        json!({ "freq": [0.0, -2.0], "unison": 7, "detune": 0.5, "sync": [0.7, 2.4] }),
    );
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$supersaw",
            json!({ "freq": poly_cable("src", LANES), "unison": 6, "fm": cable("src", 0), "sync": cable("lfo", 1) })
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

#[test]
fn supersaw_bounds_hold_under_hard_sync() {
    // Each sync reset adds a band-limiting correction to its saw; a high
    // sync restarts every saw several times a cycle.
    for freq in [-3.0, -1.0, 0.5, 3.5] {
        let params = json!({
            "freq": [freq, freq + 0.3],
            "unison": 7,
            "detune": 0.0,
            "sync": [2.6, 3.9],
            "waitForZeroCross": false,
        });
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
}
