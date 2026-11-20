//! `$saw` renders identically on the block and per-sample paths: held and
//! modulated pitch and shape, FM, phase offset and hard sync, band-limited
//! or not.

mod block_common;

use block_common::*;
use modular_core::types::ProcessingMode;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn saw_paths_agree() {
    check_paths_agree("$saw", json!({ "freq": [0.0, 0.5, -1.0, 2.0] }));
    check_paths_agree(
        "$saw",
        json!({
            "freq": poly_cable("lfo", LANES),
            "fm": cable("src", 1),
            "fmMode": "exp",
            "shape": poly_cable("lfo", LANES),
            "phaseOffset": 0.25,
        }),
    );
    check_paths_agree(
        "$saw",
        json!({
            "freq": -8.0,
            "shape": [0.0, 2.5, 5.0, 4.0],
            "waitForZeroCross": false,
        }),
    );
    check_paths_agree(
        "$saw",
        json!({ "freq": 1.0, "sync": poly_cable("src", LANES), "phaseOffset": cable("lfo", 0) }),
    );
    check_paths_agree(
        "$saw",
        json!({
            "freq": poly_cable("lfo", LANES),
            "shape": poly_cable("lfo", LANES),
            "sync": cable("src", 1),
            "bandlimit": false,
        }),
    );
}

/// Without band-limiting the saw is exact: every sample rises by the same
/// step, and each wrap falls the full 10 V in one sample.
#[test]
fn unbandlimited_saw_steps_exactly() {
    let render_lane0 = |bandlimit: bool| {
        let patch = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            "$saw",
            json!({ "freq": 2.0, "waitForZeroCross": false, "bandlimit": bandlimit }),
        );
        let (dut, _, _) = render(&patch, &[BLOCK_SIZE]);
        dut.into_iter().step_by(LANES).collect::<Vec<f32>>()
    };
    let rise = 10.0 * 261.625_58 * 4.0 / SAMPLE_RATE;
    let exact = |out: &[f32]| {
        out.windows(2).all(|w| {
            let step = w[1] - w[0];
            (step - rise).abs() < 1e-3 || (step - (rise - 10.0)).abs() < 1e-3
        })
    };
    assert!(exact(&render_lane0(false)));
    assert!(
        !exact(&render_lane0(true)),
        "the band-limited wrap is smoothed"
    );
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$saw",
            json!({ "freq": poly_cable("src", LANES), "fm": cable("src", 0), "sync": cable("src", 1) })
        ),
        0
    );
}

#[test]
fn swap_agrees_through_a_shape_glide() {
    check_swap_agrees(
        "$saw",
        json!({ "freq": [0.0, 0.5, 1.0, 1.5], "shape": 0.0 }),
        json!({ "freq": [0.0, 0.5, 1.0, 1.5], "shape": [2.5, 5.0] }),
    );
}
