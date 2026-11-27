//! `$nam` clear of the C++ runtime's heap, which the Rust allocator cannot
//! see. NeuralAmpModelerCore's tracker counts every malloc and free in the
//! process, so this binary holds one test and nothing else runs beside it.
#![cfg(not(windows))]

mod block_common;

use block_common::*;
use serde_json::json;

#[test]
fn rendering_never_allocates_in_the_runtime() {
    let seen = nam_sys::count_allocations(|| drop(std::hint::black_box(vec![0u8; 64])));
    assert!(seen > 0, "the tracker must see this binary's allocations");
    for (model, quality) in [
        ("/a2.nam", "full"),
        ("/slim.nam", "lite"),
        ("/slim_wavenet.nam", "lite"),
    ] {
        let params =
            json!({ "input": poly_cable("src", LANES), "model": model, "quality": quality });
        let count =
            counted_while_rendering("$nam", params, |render| nam_sys::count_allocations(render));
        assert_eq!(count, 0, "{model} {quality}");
    }
}
