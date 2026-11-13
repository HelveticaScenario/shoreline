//! DSL ↔ engine range parity: every output the DSL types as ranged is one
//! the engine bounds, with a range that swings, so `.range(outMin, outMax)`
//! maps from a real range.
//!
//! This test loads `fixtures/dsl_ranged.json`, emitted by the vitest file
//! `dslRangedFixture.test.ts` via `yarn gen:dsl-ranged-fixture`: a patch of
//! DSL chains and the outputs the DSL hands out as ranged. It builds the
//! patch with a `$remap` reading each of those outputs, renders it, and
//! checks the bounds of every lane at every slot.

use std::fs;
use std::path::PathBuf;

use modular_core::Patch;
use modular_core::dsp::get_params_deserializers;
use modular_core::params::DeserializedParams;
use modular_core::types::ProcessingMode;
use serde::Deserialize;
use serde_json::{Value, json};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 64;
const BLOCKS: usize = 16;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModuleSpec {
    id: String,
    module_type: String,
    params: Value,
}

#[derive(Deserialize)]
struct Output {
    module: String,
    port: String,
    lane: usize,
}

#[derive(Deserialize)]
struct Ranged {
    label: String,
    outputs: Vec<Output>,
}

#[derive(Deserialize)]
struct Fixture {
    modules: Vec<ModuleSpec>,
    ranged: Vec<Ranged>,
}

fn fixture() -> Fixture {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "dsl_ranged.json",
    ]
    .iter()
    .collect();
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn every_dsl_ranged_output_has_a_range_in_the_engine() {
    let Fixture { modules, ranged } = fixture();
    let readers = ranged
        .iter()
        .flat_map(|r| &r.outputs)
        .enumerate()
        .map(|(i, o)| {
            let input = json!({
                "type": "cable",
                "module": o.module,
                "port": o.port,
                "lane": o.lane,
            });
            (
                format!("reader_{i}"),
                "$remap".to_string(),
                json!({ "input": input, "outMin": 0.0, "outMax": 1.0 }),
            )
        });
    let specs: Vec<_> = modules
        .into_iter()
        .map(|m| (m.id, m.module_type, m.params))
        .chain(readers)
        .collect();

    let deserializers = get_params_deserializers();
    let mut patch = Patch::new();
    patch
        .insert_modules(
            specs.iter().map(|(id, module_type, params)| {
                let cached = deserializers[module_type.as_str()](params.clone())
                    .unwrap_or_else(|e| panic!("{id}: {e:?}"));
                (
                    id.clone(),
                    module_type.clone(),
                    DeserializedParams {
                        params: cached.params,
                        lane_count: cached.lane_count,
                        f64_inputs: false,
                        channels: 1,
                    },
                    ProcessingMode::Block,
                )
            }),
            SAMPLE_RATE,
            BLOCK_SIZE,
        )
        .unwrap();
    patch.connect_all();

    for _ in 0..BLOCKS {
        for module in patch.sampleables.values() {
            module.start_block();
        }
        for module in patch.sampleables.values() {
            module.ensure_processed_to(BLOCK_SIZE);
        }
        for Ranged { label, outputs } in &ranged {
            for o in outputs {
                let module = &patch.sampleables[o.module.as_str()];
                for slot in 0..BLOCK_SIZE {
                    let bounds = module.get_bounds_at(&o.port, o.lane, slot);
                    assert!(
                        bounds.is_bounded() && bounds.swings(),
                        "{label}: {}:{} lane {} reads {bounds:?} at slot {slot}",
                        o.module,
                        o.port,
                        o.lane,
                    );
                }
            }
        }
    }
}
