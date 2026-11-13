//! Cables to a Block-mode producer read its block buffer directly once a slot
//! is computed, and fall back to `get_value_at` otherwise. These tests pin
//! that the direct read is sample-identical to the `get_value_at` path, in
//! either processing order.

use modular_core::Patch;
use modular_core::dsp::get_params_deserializers;
use modular_core::params::DeserializedParams;
use modular_core::types::ProcessingMode;
use serde_json::{Value, json};

const SAMPLE_RATE: f32 = 48000.0;
const BLOCK_SIZE: usize = 64;
const LANES: usize = 4;

fn poly_cable(module: &str, lanes: usize) -> Value {
    Value::Array(
        (0..lanes)
            .map(
                |lane| json!({ "type": "cable", "module": module, "port": "output", "lane": lane }),
            )
            .collect(),
    )
}

/// `src` (poly saw) feeds `dst` (lowpass) over one cable per lane. `dst`
/// also reads lane 1 of `src` as a mono cutoff, exercising the lane
/// offset on a narrower consumer.
fn build(src_mode: ProcessingMode) -> Patch {
    let deserializers = get_params_deserializers();
    let freqs: Vec<f32> = (0..LANES).map(|lane| lane as f32 * 0.3).collect();
    let modules = [
        ("src", "$saw", json!({ "freq": freqs }), src_mode),
        (
            "dst",
            "$lpf",
            json!({
                "input": poly_cable("src", LANES),
                "cutoff": { "type": "cable", "module": "src", "port": "output", "lane": 1 },
                "resonance": 1.0,
            }),
            ProcessingMode::Block,
        ),
    ];
    let mut patch = Patch::new();
    patch
        .insert_modules(
            modules.into_iter().map(|(id, module_type, params, mode)| {
                let cached = deserializers[module_type](params).unwrap();
                (
                    id.to_string(),
                    module_type.to_string(),
                    DeserializedParams {
                        params: cached.params,
                        lane_count: cached.lane_count,
                        f64_inputs: false,
                        channels: 1,
                    },
                    mode,
                )
            }),
            SAMPLE_RATE,
            BLOCK_SIZE,
        )
        .unwrap();
    patch.connect_all();
    patch
}

/// Render `blocks` blocks, processing modules in `order`, and collect every
/// lane of `dst`'s output.
fn render(patch: &Patch, order: &[&str], blocks: usize) -> Vec<f32> {
    let dst = &patch.sampleables["dst"];
    let mut out = Vec::new();
    for _ in 0..blocks {
        for module in patch.sampleables.values() {
            module.start_block();
        }
        for id in order {
            patch.sampleables[*id].ensure_processed_to(BLOCK_SIZE);
        }
        for index in 0..BLOCK_SIZE {
            for lane in 0..LANES {
                out.push(dst.get_value_at("output", lane, index));
            }
        }
    }
    out
}

#[test]
fn port_view_only_for_block_mode() {
    let block = build(ProcessingMode::Block);
    let view = block.sampleables["src"]
        .port_view("output")
        .expect("Block-mode wrapper offers a view");
    assert_eq!(view.lanes, LANES);
    assert!(block.sampleables["src"].port_view("nonexistent").is_none());

    let sample = build(ProcessingMode::Sample);
    assert!(sample.sampleables["src"].port_view("output").is_none());
}

#[test]
fn direct_reads_match_get_value_at_reads() {
    // A Sample-mode producer offers no view, so every read goes through
    // `get_value_at`.
    let reference = render(&build(ProcessingMode::Sample), &["src", "dst"], 8);

    // Producer first: every consumer read hits an already-computed slot.
    let producer_first = render(&build(ProcessingMode::Block), &["src", "dst"], 8);
    assert_eq!(producer_first, reference);

    // Consumer first: reads find unrendered slots and pull the producer.
    let consumer_first = render(&build(ProcessingMode::Block), &["dst", "src"], 8);
    assert_eq!(consumer_first, reference);
}
