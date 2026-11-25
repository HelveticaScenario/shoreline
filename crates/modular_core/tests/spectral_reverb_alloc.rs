//! The spectral reverbs transform a frame every few hundred samples; those
//! frames, and a control change that rebuilds their per-bin tables, must not
//! touch the heap.

use std::collections::HashMap;

use modular_core::types::{ModuleSpec, PatchGraph};
use serde_json::{Value, json};

mod block_common;
mod common;
use block_common::{CountingAllocator, allocations_while, cable};
use common::from_graph;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

const SAMPLE_RATE: f32 = 48_000.0;
/// Long enough for dozens of frames on each side.
const FRAMES: usize = 30_000;

fn allocations(effect: &str, mut params: Value) -> usize {
    params["input"] = cable("src", 0);
    let module = |id: &str, module_type: &str, params: Value| ModuleSpec {
        id: id.to_string(),
        module_type: module_type.to_string(),
        id_is_explicit: None,
        skip_state_transfer: None,
        params,
    };
    let graph = PatchGraph {
        modules: vec![
            module("src", "$saw", json!({ "freq": 0.0 })),
            module("lfo", "$sine", json!({ "freq": -6.0 })),
            module("fx", effect, params),
        ],
        module_id_remaps: None,
        scopes: vec![],
        vu_meters: vec![],
    };
    let patch = from_graph(&graph, SAMPLE_RATE, 1, &HashMap::new()).unwrap();
    let fx = &patch.sampleables["fx"];
    let mut heard = 0.0;
    let count = allocations_while(|| {
        for _ in 0..FRAMES {
            for module in patch.sampleables.values() {
                module.start_block();
            }
            for module in patch.sampleables.values() {
                module.ensure_processed();
            }
            heard += fx.get_value_at("output", 0, 0).abs();
        }
    });
    assert!(heard > 0.0, "{effect} should sound");
    count
}

#[test]
fn the_spectral_reverbs_render_without_allocating() {
    for effect in ["$spectralReverb.wash", "$spectralReverb.echo"] {
        let steady = json!({ "blur": 2.0, "shimmer": 1.0 });
        assert_eq!(allocations(effect, steady), 0, "{effect}");
        // A moving decay rebuilds the per-bin tables on every frame.
        let moving = json!({ "decay": cable("lfo", 0), "blur": 2.0 });
        assert_eq!(allocations(effect, moving), 0, "{effect}, moving decay");
    }
}
