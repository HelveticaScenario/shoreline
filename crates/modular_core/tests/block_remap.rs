//! `$remap` maps its input through its range identically on the block and
//! per-sample paths.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn remap_through_a_per_lane_range() {
    check_module(
        "$remap",
        json!({
            "input": poly_cable("src", LANES),
            "inMin": -5.0,
            "inMax": 5.0,
            "outMin": [-1.0, -2.0, 0.0, 1.0],
            "outMax": cable("lfo", 2),
        }),
        |src, lfo| {
            let out_min: Vec<f32> = (0..src.len())
                .map(|k| [-1.0, -2.0, 0.0, 1.0][k % LANES])
                .collect();
            let out_max: Vec<f32> = (0..src.len()).map(|k| lfo[k - k % LANES + 2]).collect();
            src.iter()
                .zip(out_min.iter().zip(&out_max))
                .map(|(&x, (&out_min, &out_max))| {
                    let v = (x + 5.0) * (out_max - out_min) / 10.0 + out_min;
                    if v.is_finite() { v } else { 0.0 }
                })
                .collect()
        },
    );
}

#[test]
fn paths_agree() {
    check_paths_agree(
        "$remap",
        json!({
            "input": poly_cable("src", LANES),
            "inMin": cable("lfo", 1),
            "inMax": 5.0,
            "outMin": poly_cable("lfo", LANES),
            "outMax": [3.0, 4.0],
        }),
    );
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$remap",
            json!({ "input": poly_cable("src", LANES), "inMin": -5.0, "inMax": 5.0, "outMin": -1.0, "outMax": cable("src", 3) })
        ),
        0
    );
}

#[test]
fn bounds_are_the_output_range_in_order() {
    let bounds = check_bounds_agree(
        "$remap",
        json!({ "input": poly_cable("src", LANES), "outMin": 5.0, "outMax": 0.0, "inMin": -5.0, "inMax": 5.0 }),
    );
    assert!(bounds.iter().all(|&b| b == Bounds::new(5.0, 0.0)));
    // A moving output range moves the bounds sample by sample.
    let bounds = check_bounds_agree(
        "$remap",
        json!({ "input": poly_cable("src", LANES), "outMin": 1.0, "outMax": poly_cable("lfo", LANES), "inMin": -5.0, "inMax": 5.0 }),
    );
    assert!(bounds.windows(LANES + 1).any(|w| w[0] != w[LANES]));
}

#[test]
fn setting_an_unset_input_range_glides_from_the_one_in_use() {
    let input = poly_cable("src", LANES);
    check_swap_agrees(
        "$remap",
        json!({ "input": input, "outMin": 0.0, "outMax": 1.0 }),
        json!({ "input": input, "outMin": 0.0, "outMax": 1.0, "inMin": -2.0, "inMax": 2.0 }),
    );
}
