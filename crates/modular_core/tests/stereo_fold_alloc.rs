//! `$stereoFold` renders without touching the heap.

mod block_common;

use block_common::*;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn allocation_free() {
    // Two notes of two copies, the width modulated per note.
    assert_eq!(
        allocations_while_rendering(
            "$stereoFold",
            json!({ "input": poly_cable("src", LANES), "voices": 2, "width": poly_cable("lfo", 2) })
        ),
        0
    );
}
