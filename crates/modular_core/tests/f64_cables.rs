//! f64 outputs: the clock's playhead keeps sub-sample precision far into a
//! song, and modules whose outputs inherit precision carry it through
//! arithmetic, while f32 readers see the nearest f32.

use std::collections::HashSet;

use modular_core::params::DeserializedParams;
use modular_core::patch::Patch;
use modular_core::types::{ProcessingMode, ROOT_CLOCK_ID};
use serde_json::{Value, json};

const SAMPLE_RATE: f32 = 48_000.0;
/// About 4.8 samples per bar in 4/4, so a test reaches bar 1000 quickly.
const TEMPO: f64 = 2_400_000.0;

fn playhead() -> Value {
    json!({ "type": "cable", "module": *ROOT_CLOCK_ID, "port": "playhead", "channel": 0 })
}

fn cable(module: &str) -> Value {
    json!({ "type": "cable", "module": module, "port": "output", "channel": 0 })
}

/// The clock plus `modules`; those named in `f64_inputs` read an f64 output,
/// as the app's precision analysis would mark them.
fn build(
    modules: &[(&str, &str, Value)],
    f64_inputs: &[&str],
    clock_mode: ProcessingMode,
    block_size: usize,
) -> Patch {
    let deserializers = modular_core::dsp::get_params_deserializers();
    let f64_inputs: HashSet<&str> = f64_inputs.iter().copied().collect();
    let clock = (
        ROOT_CLOCK_ID.as_str(),
        "_clock",
        json!({ "tempo": TEMPO, "numerator": 4, "denominator": 4 }),
    );
    let specs = std::iter::once(&clock)
        .chain(modules)
        .map(|(id, module_type, params)| {
            let cached = deserializers[*module_type](params.clone()).expect("params deserialize");
            let mode = if *id == ROOT_CLOCK_ID.as_str() {
                clock_mode
            } else {
                ProcessingMode::Block
            };
            (
                id.to_string(),
                module_type.to_string(),
                DeserializedParams {
                    params: cached.params,
                    channel_count: cached.channel_count,
                    f64_inputs: f64_inputs.contains(id),
                },
                mode,
            )
        });
    let mut patch = Patch::new();
    patch
        .insert_modules(specs.collect::<Vec<_>>(), SAMPLE_RATE, block_size)
        .expect("modules construct");
    patch.rebuild_message_listeners();
    patch.connect_all();
    patch
}

fn process_block(patch: &Patch) {
    for module in patch.sampleables.values() {
        module.start_block();
    }
    for module in patch.sampleables.values() {
        module.ensure_processed();
    }
}

fn read_f64(patch: &Patch, id: &str, port: &str, slot: usize) -> f64 {
    patch.sampleables[id].get_value_at_f64(port, 0, slot)
}

fn read_f32(patch: &Patch, id: &str, port: &str, slot: usize) -> f32 {
    patch.sampleables[id].get_value_at(port, 0, slot)
}

/// Run `patch` until the playhead passes `bars`, checking `check` on every
/// slot from then on, for one more block.
fn run_past(patch: &Patch, block_size: usize, bars: f64, mut check: impl FnMut(&Patch, usize)) {
    let mut checked = false;
    for _ in 0..1_000_000 {
        process_block(patch);
        if read_f64(patch, &ROOT_CLOCK_ID, "playhead", 0) > bars {
            for slot in 0..block_size {
                check(patch, slot);
            }
            if checked {
                return;
            }
            checked = true;
        }
    }
    panic!("playhead never passed bar {bars}");
}

#[test]
fn playhead_and_inheriting_modules_keep_f64_precision() {
    let modules = [
        (
            "sub",
            "$sub",
            json!({ "input": playhead(), "offset": 1000.0 }),
        ),
        (
            "frac",
            "$wrap",
            json!({ "input": cable("sub"), "min": 0.0, "max": 1.0 }),
        ),
        (
            "bar",
            "_round",
            json!({ "input": playhead(), "mode": "floor" }),
        ),
        (
            "half",
            "$amp",
            json!({ "input": playhead(), "factor": 2.5 }),
        ),
        (
            "early",
            "$add",
            json!({ "input": playhead(), "offset": -3.0 }),
        ),
        (
            "double",
            "$mul",
            json!({ "input": playhead(), "factor": 2.0 }),
        ),
        (
            "quarter",
            "$div",
            json!({ "input": playhead(), "divisor": 4.0 }),
        ),
        (
            "eighth",
            "$gain",
            json!({ "input": playhead(), "level": 2.5 }),
        ),
        (
            "plain",
            "$sub",
            json!({ "input": playhead(), "offset": 1000.0 }),
        ),
    ];
    let f64_inputs = [
        "sub", "frac", "bar", "half", "early", "double", "quarter", "eighth",
    ];
    for (clock_mode, block_size) in [(ProcessingMode::Sample, 1), (ProcessingMode::Block, 64)] {
        let patch = build(&modules, &f64_inputs, clock_mode, block_size);
        let mut f32_misses = 0usize;
        run_past(&patch, block_size, 1000.0, |patch, slot| {
            let bars = read_f64(patch, &ROOT_CLOCK_ID, "playhead", slot);
            let close = |id: &str, expected: f64| {
                let got = read_f64(patch, id, "output", slot);
                assert!(
                    (got - expected).abs() < 1e-9,
                    "{id} at bar {bars} ({clock_mode:?}): {got} != {expected}"
                );
            };
            close("sub", bars - 1000.0);
            close("frac", (bars - 1000.0).rem_euclid(1.0));
            close("bar", bars.floor());
            close("half", bars * 0.5);
            close("early", bars - 3.0);
            close("double", bars * 2.0);
            close("quarter", bars * 0.25);
            close("eighth", bars * 0.125);

            // An f32 reader gets the nearest f32 of each f64 output.
            let sub = read_f64(patch, "sub", "output", slot);
            assert_eq!(read_f32(patch, "sub", "output", slot), sub as f32);

            // A module not marked as reading f64 computes in f32.
            let plain = read_f64(patch, "plain", "output", slot);
            assert_eq!(plain, ((bars as f32) - 1000.0) as f64);
            if (plain - sub).abs() > 1e-6 {
                f32_misses += 1;
            }
        });
        assert!(
            f32_misses > 0,
            "f32 arithmetic should lose the precision f64 keeps"
        );
    }
}

#[test]
fn f64_cable_read_by_an_f32_block_module() {
    // `$amp` without f64 inputs reads the playhead through its block path at
    // f32.
    let modules = [(
        "gain",
        "$amp",
        json!({ "input": playhead(), "factor": 5.0 }),
    )];
    let patch = build(&modules, &[], ProcessingMode::Block, 64);
    run_past(&patch, 64, 10.0, |patch, slot| {
        let bars = read_f64(patch, &ROOT_CLOCK_ID, "playhead", slot);
        assert_eq!(read_f32(patch, "gain", "output", slot), bars as f32);
    });
}
