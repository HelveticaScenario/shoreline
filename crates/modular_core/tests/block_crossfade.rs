//! `$crossfade` through the block-input resolution paths and the per-sample
//! `update` path, pinned to its formula.

mod block_common;

use block_common::*;
use modular_core::types::{ProcessingMode, SameIds};
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn expected(a: &[f32], b: &[f32], mix: &[f32]) -> Vec<f32> {
    a.iter()
        .zip(b)
        .zip(mix)
        .map(|((a, b), mix)| {
            let t = (mix / 5.0).clamp(0.0, 1.0);
            a * (1.0 - t) + b * t
        })
        .collect()
}

#[test]
fn constant_mix() {
    check_module(
        "$crossfade",
        json!({ "a": poly_cable("src", LANES), "b": poly_cable("lfo", LANES), "mix": 1.5 }),
        |src, lfo| expected(src, lfo, &constant(1.5)),
    );
}

#[test]
fn modulated_mix_holds_past_its_ends() {
    // The lfo swings ±5 V, so its scaled copy runs past both ends of the mix.
    check_module(
        "$crossfade",
        json!({
            "a": poly_cable("src", LANES),
            "b": 1.0,
            "mix": poly_cable("lfo", LANES),
        }),
        |src, lfo| expected(src, &constant(1.0), lfo),
    );
}

#[test]
fn narrower_leg_cycles_across_the_wider() {
    check_module(
        "$crossfade",
        json!({ "a": cable("src", 2), "b": poly_cable("lfo", LANES), "mix": 2.5 }),
        |src, lfo| {
            let a: Vec<f32> = (0..src.len()).map(|k| src[k / LANES * LANES + 2]).collect();
            expected(&a, lfo, &constant(2.5))
        },
    );
}

#[test]
fn mix_wider_than_both_legs() {
    check_module(
        "$crossfade",
        json!({ "a": cable("src", 0), "b": cable("lfo", 1), "mix": [0.0, 1.25, 2.5, 5.0] }),
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
        "$crossfade",
        json!({ "a": poly_cable("src", LANES), "b": poly_cable("lfo", LANES), "mix": cable("lfo", 1) }),
    );
}

#[test]
fn swap_agrees_through_a_mix_glide() {
    let legs = (poly_cable("src", LANES), poly_cable("lfo", LANES));
    check_swap_agrees(
        "$crossfade",
        json!({ "a": legs.0.clone(), "b": legs.1.clone(), "mix": 1.0 }),
        json!({ "a": legs.0, "b": legs.1, "mix": [4.0, 0.5] }),
    );
}

#[test]
fn an_edit_from_past_an_end_moves_at_once() {
    // A mix of 10 holds fully at `b`; editing it to 0 starts fading back to `a`
    // from the glide's first step instead of waiting for it to pass 5.
    let params = |mix: f32| json!({ "a": 0.0, "b": vec![1.0; LANES], "mix": mix });
    let old = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        "$crossfade",
        params(10.0),
    );
    let (before, _, _) = render(&old, &[BLOCK_SIZE]);
    assert_eq!(before.last(), Some(&1.0));
    let new = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        "$crossfade",
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
fn a_glide_onto_a_moving_mix_stays_between_a_and_b() {
    // Swapping a held mix for an audio-rate saw starts a glide whose offset
    // rides on the moving position; the blend still never passes either input.
    let params = |mix: serde_json::Value| json!({ "a": 0.0, "b": vec![1.0; LANES], "mix": mix });
    let old = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        "$crossfade",
        params(json!(5.0)),
    );
    render(&old, &[BLOCK_SIZE]);
    let new = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        "$crossfade",
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
        json!({ "a": poly_cable("src", LANES), "b": 0.0, "mix": 2.5 }),
        json!({ "a": cable("src", 1), "b": poly_cable("src", LANES), "mix": [cable("src", 0), 2.0] }),
    ] {
        assert_eq!(allocations_while_rendering("$crossfade", params), 0);
    }
}

#[test]
fn bounds_span_both_legs() {
    let bounds = check_bounds_agree(
        "$crossfade",
        json!({ "a": poly_cable("src", LANES), "b": 8.0, "mix": poly_cable("lfo", LANES) }),
    );
    assert!(bounds.iter().all(|&b| b == Bounds::new(-5.0, 8.0)));
}
