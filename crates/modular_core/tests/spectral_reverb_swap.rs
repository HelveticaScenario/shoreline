//! A slider drag sends a patch update every few milliseconds, each one
//! rebuilding the module and swapping its state in. The spectral reverbs'
//! random phases must run straight through those updates: a patch that
//! changes nothing renders exactly as one that is never updated.

use std::collections::HashMap;

use modular_core::dsp::utils::rng::TEST_SEED_BASE;
use modular_core::patch::Patch;
use modular_core::types::{ModuleSpec, PatchGraph, SameIds};
use serde_json::json;

mod common;
use common::from_graph;

const SAMPLE_RATE: f32 = 48_000.0;
const FRAMES: usize = 24_000;
/// Samples between updates, about what a slider drag sends.
const UPDATE_EVERY: usize = 480;

fn patch(effect: &str) -> Patch {
    let module = |id: &str, module_type: &str, params| ModuleSpec {
        id: id.to_string(),
        module_type: module_type.to_string(),
        id_is_explicit: None,
        skip_state_transfer: None,
        params,
    };
    let graph = PatchGraph {
        modules: vec![
            module("src", "$saw", json!({ "freq": 0.0 })),
            module(
                "fx",
                effect,
                json!({ "input": { "type": "cable", "module": "src", "port": "output", "lane": 0 }, "decay": 4.0, "blur": 2.0 }),
            ),
        ],
        module_id_remaps: None,
        scopes: vec![],
        vu_meters: vec![],
    };
    from_graph(&graph, SAMPLE_RATE, 1, &HashMap::new()).unwrap()
}

fn render(effect: &str, update: bool) -> Vec<f32> {
    TEST_SEED_BASE.set(Some(0x5EED));
    let mut current = patch(effect);
    let mut out = Vec::with_capacity(FRAMES * 2);
    for n in 0..FRAMES {
        if update && n % UPDATE_EVERY == 0 {
            let next = patch(effect);
            for (id, module) in &next.sampleables {
                module.transfer_state_from(current.sampleables[id].as_ref(), &SameIds);
            }
            next.connect_all();
            current = next;
        }
        for module in current.sampleables.values() {
            module.start_block();
        }
        for module in current.sampleables.values() {
            module.ensure_processed();
        }
        let fx = &current.sampleables["fx"];
        out.extend([
            fx.get_value_at("output", 0, 0),
            fx.get_value_at("output", 1, 0),
        ]);
    }
    out
}

#[test]
fn patch_updates_leave_the_tail_untouched() {
    for effect in ["$spectralReverb.wash", "$spectralReverb.echo"] {
        let steady = render(effect, false);
        assert!(steady.iter().any(|&s| s != 0.0), "{effect} should sound");
        assert!(render(effect, true) == steady, "{effect}");
    }
}
