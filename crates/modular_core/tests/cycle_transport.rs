//! A `$cycle` on the global clock releases its notes when the transport stops
//! and plays again from the top when it starts; one on its own `playhead`
//! ignores the transport.

use std::collections::HashMap;

use modular_core::patch::Patch;
use modular_core::types::{ClockMessages, Message, ModuleSpec, PatchGraph};
use serde_json::{Value, json};

mod common;
use common::from_graph;

const SAMPLE_RATE: f32 = 48000.0;
/// 48000 BPM in 4/4 at 48 kHz.
const SAMPLES_PER_CYCLE: usize = 240;

fn mini(source: &str) -> Value {
    let parsed = modular_core::dsp::seq::seq_value::ParsedPatternPayload::parse_for_test(source);
    serde_json::to_value(&parsed).expect("payload should serialize")
}

fn cycle_patch(playhead: Option<f64>) -> Patch {
    let mut params = json!({ "pattern": mini("c4 e4") });
    if let Some(playhead) = playhead {
        params["playhead"] = json!(playhead);
    }
    let graph = PatchGraph {
        modules: [
            (
                "ROOT_CLOCK",
                "_clock",
                json!({ "tempo": 48000.0, "numerator": 4, "denominator": 4 }),
            ),
            ("seq", "$cycle", params),
        ]
        .into_iter()
        .map(|(id, module_type, params)| ModuleSpec {
            id: id.to_string(),
            module_type: module_type.to_string(),
            id_is_explicit: None,
            skip_state_transfer: None,
            params,
        })
        .collect(),
        module_id_remaps: None,
        scopes: vec![],
        vu_meters: vec![],
    };
    from_graph(&graph, SAMPLE_RATE, 1, &HashMap::new()).expect("from_graph failed")
}

/// The gate of `seq` on each of `frames` samples.
fn gates(patch: &Patch, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|_| {
            for module in patch.sampleables.values() {
                module.start_block();
            }
            for module in patch.sampleables.values() {
                module.ensure_processed();
            }
            patch
                .sampleables
                .get("seq")
                .unwrap()
                .get_value_at("gate", 0, 0)
        })
        .collect()
}

#[test]
fn transport_stop_releases_the_notes_of_a_cycle_on_the_global_clock() {
    let mut patch = cycle_patch(None);
    patch
        .dispatch_message(&Message::Clock(ClockMessages::Start))
        .unwrap();
    assert!(gates(&patch, SAMPLES_PER_CYCLE).iter().any(|&g| g > 0.0));

    patch
        .dispatch_message(&Message::Clock(ClockMessages::Stop))
        .unwrap();
    assert!(
        gates(&patch, 4 * SAMPLES_PER_CYCLE)
            .iter()
            .all(|&g| g == 0.0)
    );

    patch
        .dispatch_message(&Message::Clock(ClockMessages::Start))
        .unwrap();
    // A gate opens a minimum gate length after its onset.
    assert!(
        gates(&patch, SAMPLES_PER_CYCLE / 4)
            .iter()
            .any(|&g| g > 0.0),
        "the first note plays from the top"
    );
}

#[test]
fn a_cycle_on_its_own_playhead_plays_through_a_transport_stop() {
    let mut patch = cycle_patch(Some(0.1));
    assert!(gates(&patch, 100)[50..].iter().all(|&g| g > 0.0));

    patch
        .dispatch_message(&Message::Clock(ClockMessages::Stop))
        .unwrap();
    assert!(gates(&patch, SAMPLES_PER_CYCLE).iter().all(|&g| g > 0.0));
}
