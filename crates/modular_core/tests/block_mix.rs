//! `$mix` folds its inputs and applies its gain identically on the block
//! and per-sample paths, in every mode.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn mix_every_mode_across_input_widths() {
    // A four-wide cable, a two-wide cable and a mono constant: channels past an
    // input's width leave it out. Gain is a mono cable.
    for mode in ["sum", "average", "max", "min"] {
        check_module(
            "$mix",
            json!({
                "inputs": [poly_cable("src", CHANNELS), poly_cable("lfo", 2), 0.0],
                "mode": mode,
                "gain": cable("lfo", 3),
            }),
            |src, lfo| {
                (0..src.len())
                    .map(|k| {
                        let ch = k % CHANNELS;
                        let mut values = vec![src[k]];
                        if ch < 2 {
                            values.push(lfo[k]);
                        }
                        if ch < 1 {
                            values.push(0.0);
                        }
                        let pre = match mode {
                            "sum" => values.iter().fold(0.0, |a, v| a + v),
                            "average" => {
                                values.iter().fold(0.0, |a, v| a + v) / values.len() as f32
                            }
                            "max" => {
                                let (mut best_abs, mut best) = (-1.0f32, 0.0);
                                for &v in &values {
                                    if v.abs() > best_abs {
                                        (best_abs, best) = (v.abs(), v);
                                    }
                                }
                                if best_abs >= 0.0 { best } else { 0.0 }
                            }
                            _ => {
                                let (mut best_abs, mut best) = (f32::INFINITY, 0.0);
                                for &v in &values {
                                    if v != 0.0 && v.abs() < best_abs {
                                        (best_abs, best) = (v.abs(), v);
                                    }
                                }
                                if best_abs.is_finite() { best } else { 0.0 }
                            }
                        };
                        let amp = lfo[k - ch + 3];
                        let g = amp * (1.0 / 5.0);
                        let v = pre * (g * g * g);
                        if v.is_finite() { v } else { 0.0 }
                    })
                    .collect()
            },
        );
    }
}

#[test]
fn paths_agree() {
    for mode in ["sum", "average", "max", "min"] {
        check_paths_agree(
            "$mix",
            json!({
                "inputs": [poly_cable("src", CHANNELS), cable("lfo", 1), [0.5, -0.25]],
                "mode": mode,
                "gain": [cable("lfo", 0), 4.0],
            }),
        );
    }
    check_paths_agree(
        "$mix",
        json!({ "inputs": [poly_cable("src", 2)], "gain": poly_cable("lfo", CHANNELS) }),
    );
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$mix",
            json!({ "inputs": [poly_cable("src", CHANNELS), cable("src", 1), 0.5], "mode": "max", "gain": cable("src", 0) })
        ),
        0
    );
}

#[test]
fn swap_agrees_through_a_gain_glide() {
    let inputs = json!([poly_cable("src", CHANNELS), cable("lfo", 1)]);
    check_swap_agrees(
        "$mix",
        json!({ "inputs": inputs.clone(), "gain": 1.0 }),
        json!({ "inputs": inputs, "gain": [4.0, 6.0] }),
    );
}
