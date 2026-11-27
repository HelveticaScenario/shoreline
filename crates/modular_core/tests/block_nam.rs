//! `$nam` through the block and per-sample paths, across patch swaps, and
//! clear of the Rust heap.

mod block_common;

use block_common::*;
use modular_core::types::{ProcessingMode, SameIds};
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn params() -> serde_json::Value {
    json!({ "input": poly_cable("src", LANES), "model": "/a2.nam" })
}

#[test]
fn paths_agree() {
    check_paths_agree("$nam", params());
}

#[test]
fn modulated_levels_agree() {
    check_paths_agree(
        "$nam",
        json!({
            "input": poly_cable("src", LANES),
            "model": "/a2.nam",
            "gain": poly_cable("lfo", LANES),
            "level": poly_cable("lfo", LANES),
        }),
    );
}

#[test]
fn level_edits_glide_alike() {
    check_swap_agrees(
        "$nam",
        params(),
        json!({
            "input": poly_cable("src", LANES),
            "model": "/a2.nam",
            "gain": 12.0,
            "level": -6.0,
        }),
    );
}

/// An unrelated edit keeps each lane's model and its history: the swapped
/// patch renders on exactly as the unswapped one does.
#[test]
fn a_swap_keeps_the_running_model() {
    let render_two = |swap: bool| {
        let first = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            "$nam",
            params(),
        );
        let (mut out, _, _) = render(&first, &[BLOCK_SIZE]);
        let second = if swap {
            let next = build(
                ProcessingMode::Block,
                ProcessingMode::Block,
                "$nam",
                params(),
            );
            for (id, module) in &next.sampleables {
                module.transfer_state_from(first.sampleables[id].as_ref(), &SameIds);
            }
            next.connect_all();
            next
        } else {
            first
        };
        out.extend(render(&second, &[BLOCK_SIZE]).0);
        out
    };
    assert_eq!(render_two(true), render_two(false));
}

#[test]
fn a_missing_model_is_silent() {
    let patch = build(
        ProcessingMode::Block,
        ProcessingMode::Block,
        "$nam",
        json!({ "input": poly_cable("src", LANES), "model": "/none.nam" }),
    );
    let (out, _, _) = render(&patch, &[BLOCK_SIZE]);
    assert!(out.iter().all(|&v| v == 0.0));
}

#[test]
fn rendering_never_allocates() {
    assert_eq!(allocations_while_rendering("$nam", params()), 0);
}

fn slim(quality: &str) -> serde_json::Value {
    json!({ "input": poly_cable("src", LANES), "model": "/slim.nam", "quality": quality })
}

#[test]
fn lite_paths_agree() {
    check_paths_agree("$nam", slim("lite"));
}

#[test]
fn lite_runs_the_other_version() {
    let sound = |quality: &str| {
        let patch = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            "$nam",
            slim(quality),
        );
        render(&patch, &[BLOCK_SIZE]).0
    };
    assert_ne!(sound("lite"), sound("full"));
}

/// A quality edit swaps in a model of the new quality.
#[test]
fn a_quality_edit_swaps_the_model() {
    let first = build(
        ProcessingMode::Block,
        ProcessingMode::Block,
        "$nam",
        slim("full"),
    );
    render(&first, &[BLOCK_SIZE]);
    let next = build(
        ProcessingMode::Block,
        ProcessingMode::Block,
        "$nam",
        slim("lite"),
    );
    for (id, module) in &next.sampleables {
        module.transfer_state_from(first.sampleables[id].as_ref(), &SameIds);
    }
    next.connect_all();
    assert_ne!(
        render(&next, &[BLOCK_SIZE]).0,
        render(&first, &[BLOCK_SIZE]).0
    );
}

#[test]
fn only_file_reading_modules_prepare_resources() {
    let patch = build(
        ProcessingMode::Block,
        ProcessingMode::Block,
        "$nam",
        params(),
    );
    assert!(patch.sampleables["dut"].prepares_resources());
    assert!(!patch.sampleables["src"].prepares_resources());
}
