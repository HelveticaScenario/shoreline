//! `$softClip` renders each output the same whichever of the others is read.

use serde_json::json;

mod block_common;
use block_common::{LANES, check_connections_agree, poly_cable};

#[test]
fn outputs_render_the_same_whichever_others_are_read() {
    for knee in [0.0, 2.0, 5.0] {
        check_connections_agree(
            "$softClip",
            json!({ "input": poly_cable("src", LANES), "ceiling": 3.0, "knee": knee }),
            &["clip", "tanh"],
            1e-4,
            64,
        );
    }
}
