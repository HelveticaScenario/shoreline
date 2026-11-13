//! Stereo lanes: a lane carries one or two sides, settled when the patch is
//! built. A lifted module runs one lane per side, a mono one folds, and a
//! port stores lane `l` side `c` at `l * channels + c`.

use modular_core::Patch;
use modular_core::dsp::{get_params_deserializers, schema};
use modular_core::params::DeserializedParams;
use modular_core::types::{ProcessingMode, SameIds};
use serde_json::{Value, json};

const SAMPLE_RATE: f32 = 48000.0;

fn cable(module: &str, lane: usize) -> Value {
    json!({ "type": "cable", "module": module, "port": "output", "lane": lane })
}

fn build(modules: &[(&str, &str, Value)]) -> Result<Patch, String> {
    let deserializers = get_params_deserializers();
    let mut patch = Patch::new();
    patch.insert_modules(
        modules.iter().map(|(id, module_type, params)| {
            let cached = deserializers[*module_type](params.clone()).unwrap();
            (
                id.to_string(),
                module_type.to_string(),
                DeserializedParams {
                    params: cached.params,
                    lane_count: cached.lane_count,
                    f64_inputs: false,
                    channels: 1,
                },
                ProcessingMode::Sample,
            )
        }),
        SAMPLE_RATE,
        1,
    )?;
    patch.connect_all();
    Ok(patch)
}

fn default_port(module_type: &str) -> String {
    let outputs = schema()
        .into_iter()
        .find(|s| s.name == module_type)
        .unwrap()
        .outputs;
    outputs[outputs.iter().position(|o| o.default).unwrap_or(0)]
        .name
        .clone()
}

/// Render one sample and return `id`'s physical lanes.
fn step(patch: &Patch, id: &str) -> Vec<f32> {
    for module in patch.sampleables.values() {
        module.start_block();
    }
    for module in patch.sampleables.values() {
        module.ensure_processed_to(1);
    }
    let module = &patch.sampleables[id];
    let port = default_port(module.get_module_type());
    (0..module.port_lanes(&port))
        .map(|lane| module.get_value_at(&port, lane, 0))
        .collect()
}

fn signal(value: Value) -> Value {
    json!({ "value": value })
}

#[test]
fn stereo_joins_two_sides_lane_by_lane() {
    let patch = build(&[
        ("l", "$signal", signal(json!([1.0, 3.0]))),
        ("r", "$signal", signal(json!([2.0, 4.0]))),
        (
            "st",
            "$stereo",
            json!({ "left": [cable("l", 0), cable("l", 1)], "right": [cable("r", 0), cable("r", 1)] }),
        ),
    ])
    .unwrap();
    assert_eq!(step(&patch, "st"), [1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn a_lifted_module_runs_each_side() {
    let patch = build(&[
        ("l", "$signal", signal(json!(1.0))),
        ("r", "$signal", signal(json!(2.0))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("l", 0), "right": cable("r", 0) }),
        ),
        (
            "sum",
            "$add",
            json!({ "input": cable("st", 0), "offset": 10.0 }),
        ),
    ])
    .unwrap();
    assert_eq!(step(&patch, "sum"), [11.0, 12.0]);
}

#[test]
fn a_mono_operand_reaches_both_sides() {
    let patch = build(&[
        ("l", "$signal", signal(json!(1.0))),
        ("r", "$signal", signal(json!(2.0))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("l", 0), "right": cable("r", 0) }),
        ),
        ("k", "$signal", signal(json!(5.0))),
        (
            "sum",
            "$add",
            json!({ "input": cable("k", 0), "offset": cable("st", 0) }),
        ),
    ])
    .unwrap();
    assert_eq!(step(&patch, "sum"), [6.0, 7.0]);
}

#[test]
fn mono_left_and_right_fold_and_split() {
    let patch = build(&[
        ("l", "$signal", signal(json!(1.0))),
        ("r", "$signal", signal(json!(3.0))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("l", 0), "right": cable("r", 0) }),
        ),
        ("mono", "$mono", json!({ "input": cable("st", 0) })),
        ("left", "$left", json!({ "input": cable("st", 0) })),
        ("right", "$right", json!({ "input": cable("st", 0) })),
    ])
    .unwrap();
    assert_eq!(step(&patch, "mono"), [2.0]);
    assert_eq!(step(&patch, "left"), [1.0]);
    assert_eq!(step(&patch, "right"), [3.0]);
}

#[test]
fn mono_and_split_pass_a_mono_signal_unchanged() {
    let patch = build(&[
        ("k", "$signal", signal(json!([1.0, 2.0]))),
        (
            "mono",
            "$mono",
            json!({ "input": [cable("k", 0), cable("k", 1)] }),
        ),
        (
            "right",
            "$right",
            json!({ "input": [cable("k", 0), cable("k", 1)] }),
        ),
    ])
    .unwrap();
    assert_eq!(step(&patch, "mono"), [1.0, 2.0]);
    assert_eq!(step(&patch, "right"), [1.0, 2.0]);
}

#[test]
fn a_single_value_param_folds_a_stereo_signal() {
    // `$slew`'s input is read lane by lane, so the module lifts; `$keyframes`
    // reads its playhead as one value, so it stays mono and folds.
    let patch = build(&[
        ("a", "$signal", signal(json!(0.25))),
        ("b", "$signal", signal(json!(0.75))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("a", 0), "right": cable("b", 0) }),
        ),
        (
            "kf",
            "$keyframes",
            json!({ "keyframes": [[0.0, 0.0], [4.0, 1.0]], "playhead": cable("st", 0) }),
        ),
    ])
    .unwrap();
    assert_eq!(step(&patch, "kf"), [2.0]);
}

#[test]
fn a_feedback_cycle_without_a_stereo_source_stays_mono() {
    let patch = build(&[
        ("k", "$signal", signal(json!(1.0))),
        (
            "a",
            "$add",
            json!({ "input": cable("k", 0), "offset": cable("b", 0) }),
        ),
        (
            "b",
            "$add",
            json!({ "input": cable("a", 0), "offset": 0.0 }),
        ),
    ])
    .unwrap();
    assert_eq!(patch.sampleables["a"].port_lanes("output"), 1);
    assert_eq!(patch.sampleables["b"].port_lanes("output"), 1);
}

#[test]
fn a_feedback_cycle_with_a_stereo_source_is_stereo() {
    let patch = build(&[
        ("l", "$signal", signal(json!(1.0))),
        ("r", "$signal", signal(json!(2.0))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("l", 0), "right": cable("r", 0) }),
        ),
        (
            "a",
            "$add",
            json!({ "input": cable("st", 0), "offset": cable("b", 0) }),
        ),
        (
            "b",
            "$add",
            json!({ "input": cable("a", 0), "offset": 0.0 }),
        ),
    ])
    .unwrap();
    assert_eq!(patch.sampleables["a"].port_lanes("output"), 2);
    assert_eq!(patch.sampleables["b"].port_lanes("output"), 2);
}

#[test]
fn a_stereo_signal_carries_half_the_lanes() {
    let lanes: Vec<f32> = (0..33).map(|lane| lane as f32).collect();
    let error = build(&[
        ("k", "$signal", signal(json!(lanes))),
        (
            "st",
            "$stereo",
            json!({ "left": (0..33).map(|lane| cable("k", lane)).collect::<Vec<_>>(), "right": 0.0 }),
        ),
    ])
    .err()
    .unwrap();
    assert!(error.contains("st") && error.contains("32"), "{error}");
}

#[test]
fn widening_to_stereo_keeps_the_left_side_running() {
    // A slow slew settled at 0 V, then swapped to a stereo 5 V input: its left
    // side carries on from 0 V and climbs, while its right side is new and,
    // like any fresh slew, starts at its input.
    let mono = build(&[
        ("k", "$signal", signal(json!(0.0))),
        (
            "slew",
            "$slew",
            json!({ "input": cable("k", 0), "rise": 1.0 }),
        ),
    ])
    .unwrap();
    for _ in 0..16 {
        step(&mono, "slew");
    }
    let stereo = build(&[
        ("five", "$signal", signal(json!(5.0))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("five", 0), "right": cable("five", 0) }),
        ),
        (
            "slew",
            "$slew",
            json!({ "input": cable("st", 0), "rise": 1.0 }),
        ),
    ])
    .unwrap();
    for (id, module) in &stereo.sampleables {
        if let Some(old) = mono.sampleables.get(id) {
            module.transfer_state_from(old.as_ref(), &SameIds);
        }
    }
    stereo.connect_all();
    let after = step(&stereo, "slew");
    assert!(
        after[0] > 0.0 && after[0] < 0.1,
        "left carries on: {after:?}"
    );
    assert_eq!(after[1], 5.0, "right starts fresh: {after:?}");
}
