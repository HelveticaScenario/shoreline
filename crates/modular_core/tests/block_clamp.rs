//! `$clamp` holds its input within the connected bounds, identically on the block
//! and per-sample paths.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn clamp_every_bound_combination() {
    let input = poly_cable("src", LANES);
    check_module(
        "$clamp",
        json!({ "input": input.clone(), "min": -0.5, "max": cable("lfo", 0) }),
        |src, lfo| {
            let lfo0: Vec<f32> = (0..lfo.len()).map(|k| lfo[k / LANES * LANES]).collect();
            elementwise(src, &lfo0, |x, b| {
                let a = -0.5;
                let (lo, hi) = if b < a { (b, a) } else { (a, b) };
                x.clamp(lo, hi)
            })
        },
    );
    check_module(
        "$clamp",
        json!({ "input": input.clone(), "min": 0.2 }),
        |src, lfo| elementwise(src, lfo, |x, _| if x < 0.2 { 0.2 } else { x }),
    );
    check_module(
        "$clamp",
        json!({ "input": input.clone(), "max": poly_cable("lfo", LANES) }),
        |src, lfo| elementwise(src, lfo, |x, max| if x > max { max } else { x }),
    );
    check_module("$clamp", json!({ "input": input }), |src, lfo| {
        elementwise(src, lfo, |x, _| x)
    });
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$clamp",
            json!({ "input": poly_cable("src", LANES), "min": -1.0, "max": cable("src", 0) })
        ),
        0
    );
}

#[test]
fn bounds_clamp_with_the_signal() {
    let input = poly_cable("src", LANES);
    let cases = [
        (
            json!({ "input": input, "min": -1.0, "max": 2.0 }),
            Bounds::new(-1.0, 2.0),
        ),
        (json!({ "input": input, "min": 0.0 }), Bounds::new(0.0, 5.0)),
    ];
    for (params, expected) in cases {
        let bounds = check_bounds_agree("$clamp", params.clone());
        assert!(
            bounds.iter().all(|&b| b == expected),
            "{params}: {:?}",
            bounds[0]
        );
    }
    check_bounds_agree(
        "$clamp",
        json!({ "input": input, "min": poly_cable("lfo", LANES), "max": 4.0 }),
    );
}
