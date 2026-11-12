//! `_crossfade` through the block-input resolution paths and the per-sample
//! `update` path, pinned to its formula.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn expected(dry: &[f32], wet: &[f32], mix: &[f32]) -> Vec<f32> {
    dry.iter()
        .zip(wet)
        .zip(mix)
        .map(|((dry, wet), mix)| {
            let t = (mix / 5.0).clamp(0.0, 1.0);
            dry * (1.0 - t) + wet * t
        })
        .collect()
}

#[test]
fn constant_mix() {
    check_module(
        "_crossfade",
        json!({ "dry": poly_cable("src", CHANNELS), "wet": poly_cable("lfo", CHANNELS), "mix": 1.5 }),
        |src, lfo| expected(src, lfo, &constant(1.5)),
    );
}

#[test]
fn modulated_mix_holds_past_its_ends() {
    // The lfo swings ±5 V, so its scaled copy runs past both ends of the mix.
    check_module(
        "_crossfade",
        json!({
            "dry": poly_cable("src", CHANNELS),
            "wet": 1.0,
            "mix": poly_cable("lfo", CHANNELS),
        }),
        |src, lfo| expected(src, &constant(1.0), lfo),
    );
}

#[test]
fn narrower_leg_cycles_across_the_wider() {
    check_module(
        "_crossfade",
        json!({ "dry": cable("src", 2), "wet": poly_cable("lfo", CHANNELS), "mix": 2.5 }),
        |src, lfo| {
            let dry: Vec<f32> = (0..src.len())
                .map(|k| src[k / CHANNELS * CHANNELS + 2])
                .collect();
            expected(&dry, lfo, &constant(2.5))
        },
    );
}

#[test]
fn paths_agree() {
    check_paths_agree(
        "_crossfade",
        json!({ "dry": poly_cable("src", CHANNELS), "wet": poly_cable("lfo", CHANNELS), "mix": cable("lfo", 1) }),
    );
}

#[test]
fn allocation_free() {
    for params in [
        json!({ "dry": poly_cable("src", CHANNELS), "wet": 0.0, "mix": 2.5 }),
        json!({ "dry": cable("src", 1), "wet": poly_cable("src", CHANNELS), "mix": [cable("src", 0), 2.0] }),
    ] {
        assert_eq!(allocations_while_rendering("_crossfade", params), 0);
    }
}
