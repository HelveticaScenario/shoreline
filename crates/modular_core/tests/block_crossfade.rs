//! `_crossfade` through the block-input resolution paths and the per-sample
//! `update` path, pinned to its formula.

mod block_common;

use block_common::*;
use modular_core::types::{ProcessingMode, SameIds};
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
        json!({ "dry": poly_cable("src", LANES), "wet": poly_cable("lfo", LANES), "mix": 1.5 }),
        |src, lfo| expected(src, lfo, &constant(1.5)),
    );
}

#[test]
fn modulated_mix_holds_past_its_ends() {
    // The lfo swings ±5 V, so its scaled copy runs past both ends of the mix.
    check_module(
        "_crossfade",
        json!({
            "dry": poly_cable("src", LANES),
            "wet": 1.0,
            "mix": poly_cable("lfo", LANES),
        }),
        |src, lfo| expected(src, &constant(1.0), lfo),
    );
}

#[test]
fn narrower_leg_cycles_across_the_wider() {
    check_module(
        "_crossfade",
        json!({ "dry": cable("src", 2), "wet": poly_cable("lfo", LANES), "mix": 2.5 }),
        |src, lfo| {
            let dry: Vec<f32> = (0..src.len()).map(|k| src[k / LANES * LANES + 2]).collect();
            expected(&dry, lfo, &constant(2.5))
        },
    );
}

#[test]
fn mix_wider_than_both_legs() {
    check_module(
        "_crossfade",
        json!({ "dry": cable("src", 0), "wet": cable("lfo", 1), "mix": [0.0, 1.25, 2.5, 5.0] }),
        |src, lfo| {
            let leg = |values: &[f32], lane: usize| -> Vec<f32> {
                (0..values.len())
                    .map(|k| values[k / LANES * LANES + lane])
                    .collect()
            };
            let mix: Vec<f32> = (0..src.len())
                .map(|k| [0.0, 1.25, 2.5, 5.0][k % LANES])
                .collect();
            expected(&leg(src, 0), &leg(lfo, 1), &mix)
        },
    );
}

#[test]
fn paths_agree() {
    check_paths_agree(
        "_crossfade",
        json!({ "dry": poly_cable("src", LANES), "wet": poly_cable("lfo", LANES), "mix": cable("lfo", 1) }),
    );
}

#[test]
fn swap_agrees_through_a_mix_glide() {
    let legs = (poly_cable("src", LANES), poly_cable("lfo", LANES));
    check_swap_agrees(
        "_crossfade",
        json!({ "dry": legs.0.clone(), "wet": legs.1.clone(), "mix": 1.0 }),
        json!({ "dry": legs.0, "wet": legs.1, "mix": [4.0, 0.5] }),
    );
}

#[test]
fn an_edit_from_past_an_end_moves_at_once() {
    // A mix of 10 holds fully wet; editing it to 0 starts fading back to dry
    // from the glide's first step instead of waiting for it to pass 5.
    let params = |mix: f32| json!({ "dry": 0.0, "wet": vec![1.0; LANES], "mix": mix });
    let old = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        "_crossfade",
        params(10.0),
    );
    let (before, _, _) = render(&old, &[BLOCK_SIZE]);
    assert_eq!(before.last(), Some(&1.0));
    let new = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        "_crossfade",
        params(0.0),
    );
    for (id, module) in &new.sampleables {
        module.transfer_state_from(old.sampleables[id].as_ref(), &SameIds);
    }
    new.connect_all();
    let (after, _, _) = render(&new, &[BLOCK_SIZE]);
    // `render` interleaves lanes; the glide starts from the old value.
    let second = after[LANES];
    assert_eq!(after[0], 1.0);
    assert!(second < 1.0, "second sample after the edit: {second}");
    assert!(second > 0.9, "the edit glides: {second}");
}

#[test]
fn a_glide_onto_a_moving_mix_stays_between_dry_and_wet() {
    // Swapping a held mix for an audio-rate saw starts a glide whose offset
    // rides on the moving position; the blend still never passes either leg.
    let params =
        |mix: serde_json::Value| json!({ "dry": 0.0, "wet": vec![1.0; LANES], "mix": mix });
    let old = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        "_crossfade",
        params(json!(5.0)),
    );
    render(&old, &[BLOCK_SIZE]);
    let new = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        "_crossfade",
        params(poly_cable("src", LANES)),
    );
    for (id, module) in &new.sampleables {
        module.transfer_state_from(old.sampleables[id].as_ref(), &SameIds);
    }
    new.connect_all();
    let (after, _, _) = render(&new, &[BLOCK_SIZE]);
    for (k, &v) in after.iter().enumerate() {
        assert!((0.0..=1.0).contains(&v), "sample {k}: {v}");
    }
}

#[test]
fn allocation_free() {
    for params in [
        json!({ "dry": poly_cable("src", LANES), "wet": 0.0, "mix": 2.5 }),
        json!({ "dry": cable("src", 1), "wet": poly_cable("src", LANES), "mix": [cable("src", 0), 2.0] }),
    ] {
        assert_eq!(allocations_while_rendering("_crossfade", params), 0);
    }
}

#[test]
fn bounds_span_both_legs() {
    let bounds = check_bounds_agree(
        "_crossfade",
        json!({ "dry": poly_cable("src", LANES), "wet": 8.0, "mix": poly_cable("lfo", LANES) }),
    );
    assert!(bounds.iter().all(|&b| b == Bounds::new(-5.0, 8.0)));
}
