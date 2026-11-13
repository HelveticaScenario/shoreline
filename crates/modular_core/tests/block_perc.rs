//! `$perc` renders identically on the block and per-sample paths: held and
//! modulated decay times, and retriggers mid-decay and mid-attack.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn perc_paths_agree() {
    check_paths_agree("$perc", json!({ "trigger": poly_cable("src", LANES) }));
    check_paths_agree(
        "$perc",
        json!({ "trigger": poly_cable("lfo", LANES), "decay": [0.001, 0.002, 0.0005, 0.004] }),
    );
    check_paths_agree(
        "$perc",
        json!({ "trigger": poly_cable("src", LANES), "decay": poly_cable("lfo", LANES) }),
    );
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$perc",
            json!({ "trigger": poly_cable("src", LANES), "decay": cable("lfo", 0) })
        ),
        0
    );
}
