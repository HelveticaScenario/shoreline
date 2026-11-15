//! Stereo lanes: a lane carries one or two sides, settled when the patch is
//! built. A lifted module runs one lane per side, a mono one folds, and a
//! port stores lane `l` side `c` at `l * channels + c`.

use modular_core::Patch;
use modular_core::dsp::{get_constructors, get_params_deserializers, schema};
use modular_core::params::DeserializedParams;
use modular_core::patch::Settlement;
use modular_core::types::{ProcessingMode, SameIds};
use serde_json::{Value, json};

const SAMPLE_RATE: f32 = 48000.0;

fn cable(module: &str, lane: usize) -> Value {
    json!({ "type": "cable", "module": module, "port": "output", "lane": lane })
}

fn build(modules: &[(&str, &str, Value)]) -> Result<Patch, String> {
    build_settled(modules).map(|(patch, _)| patch)
}

/// `modules`, with what building them settled.
fn build_settled(modules: &[(&str, &str, Value)]) -> Result<(Patch, Settlement), String> {
    let (patch, settlement) = insert(modules)?;
    patch.connect_all();
    Ok((patch, settlement))
}

/// `modules` constructed but not yet connected, as a patch update builds
/// them before carrying state over.
fn insert(modules: &[(&str, &str, Value)]) -> Result<(Patch, Settlement), String> {
    let deserializers = get_params_deserializers();
    let mut patch = Patch::new();
    let settlement = patch.insert_modules(
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
    Ok((patch, settlement))
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
fn a_module_rebuilt_on_its_own_keeps_its_settled_channels() {
    let (mut patch, settlement) = build_settled(&[
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
    let cached =
        get_params_deserializers()["$add"](json!({ "input": cable("st", 0), "offset": 20.0 }))
            .unwrap();
    let mut params = DeserializedParams {
        params: cached.params,
        lane_count: cached.lane_count,
        f64_inputs: false,
        channels: 1,
    };
    let mode = settlement.prepare_rebuilt("sum", &mut params).unwrap();
    assert_eq!(mode, ProcessingMode::Sample);
    let rebuilt = get_constructors()["$add"]("sum", SAMPLE_RATE, params, 1, mode).unwrap();
    settlement.settle_rebuilt("sum", rebuilt.as_ref());
    patch.sampleables.insert("sum".into(), rebuilt);
    patch.connect_all();
    assert_eq!(step(&patch, "sum"), [21.0, 22.0]);
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
fn a_stereo_playhead_plays_each_side_at_its_own_position() {
    // `$keyframes` reads its playhead as a lane sum: stereo, each side its own.
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
    assert_eq!(step(&patch, "kf"), [1.0, 3.0]);
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
    // 33 mono lanes plus a stereo operand make a 33-lane stereo `$add`.
    let lanes: Vec<f32> = (0..33).map(|lane| lane as f32).collect();
    let error = build(&[
        ("k", "$signal", signal(json!(lanes))),
        ("l", "$signal", signal(json!(1.0))),
        ("st", "$stereo", json!({ "left": cable("l", 0), "right": cable("l", 0) })),
        (
            "wide",
            "$add",
            json!({ "input": (0..33).map(|lane| cable("k", lane)).collect::<Vec<_>>(), "offset": cable("st", 0) }),
        ),
    ])
    .err()
    .unwrap();
    assert!(error.contains("wide") && error.contains("32"), "{error}");
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

#[test]
fn a_side_reads_every_lane_of_a_wide_signal() {
    // 40 mono lanes into `$left`: read side by side they fill 80 physical
    // lanes, and a swap's glide marks still cover them.
    let lanes: Vec<f32> = (0..40).map(|lane| lane as f32).collect();
    let modules = [
        ("k", "$signal", signal(json!(lanes))),
        (
            "left",
            "$left",
            json!({ "input": (0..40).map(|lane| cable("k", lane)).collect::<Vec<_>>() }),
        ),
    ];
    let old = build(&modules).unwrap();
    let new = build(&modules).unwrap();
    for (id, module) in &new.sampleables {
        module.transfer_state_from(old.sampleables[id].as_ref(), &SameIds);
    }
    new.connect_all();
    let values = step(&new, "left");
    assert_eq!(values.len(), 40);
    assert_eq!(values[39], 39.0);
}

#[test]
fn stereo_folds_a_stereo_input_before_joining() {
    let patch = build(&[
        ("a", "$signal", signal(json!(1.0))),
        ("b", "$signal", signal(json!(3.0))),
        ("c", "$signal", signal(json!(5.0))),
        (
            "ab",
            "$stereo",
            json!({ "left": cable("a", 0), "right": cable("b", 0) }),
        ),
        (
            "join",
            "$stereo",
            json!({ "left": cable("ab", 0), "right": cable("c", 0) }),
        ),
    ])
    .unwrap();
    assert_eq!(step(&patch, "join"), [2.0, 5.0]);
}

#[test]
fn a_stereo_lane_filters_both_sides_with_its_own_list() {
    // Two stereo lanes with the same signal on both sides, each lane with its
    // own section list: the sides of a lane match and the lanes differ.
    let patch = build(&[
        ("a", "$signal", signal(json!([1.0, 1.0]))),
        (
            "st",
            "$stereo",
            json!({ "left": [cable("a", 0), cable("a", 1)], "right": [cable("a", 0), cable("a", 1)] }),
        ),
        (
            "eq",
            "$eq",
            json!({
                "input": [cable("st", 0), cable("st", 1)],
                "sections": [
                    [{ "type": "lowShelf", "freq": 0.0, "gain": 12.0 }],
                    [{ "type": "lowShelf", "freq": 0.0, "gain": -12.0 }],
                ],
            }),
        ),
    ])
    .unwrap();
    let mut out = Vec::new();
    for _ in 0..4800 {
        out = step(&patch, "eq");
    }
    assert_eq!(out.len(), 4);
    assert_eq!(out[0], out[1], "lane 0's sides");
    assert_eq!(out[2], out[3], "lane 1's sides");
    assert!(out[0] > out[2], "each lane its own list: {out:?}");
}

#[test]
fn set_lanes_folds_each_side_on_its_own() {
    let patch = build(&[
        ("l", "$signal", signal(json!([1.0, 2.0]))),
        ("r", "$signal", signal(json!([10.0, 20.0]))),
        (
            "st",
            "$stereo",
            json!({ "left": [cable("l", 0), cable("l", 1)], "right": [cable("r", 0), cable("r", 1)] }),
        ),
        (
            "fold",
            "$setLanes",
            json!({ "input": [cable("st", 0), cable("st", 1)], "lanes": 1 }),
        ),
    ])
    .unwrap();
    assert_eq!(step(&patch, "fold"), [3.0, 30.0]);
}

#[test]
fn a_stereo_lane_sum_makes_a_lifted_module_stereo() {
    // A mono input with a stereo band frequency: each side filters with its
    // own side's frequency.
    let patch = build(&[
        ("in", "$signal", signal(json!(1.0))),
        ("lo", "$signal", signal(json!(-3.0))),
        ("hi", "$signal", signal(json!(3.0))),
        (
            "freq",
            "$stereo",
            json!({ "left": cable("lo", 0), "right": cable("hi", 0) }),
        ),
        (
            "eq",
            "$eq",
            json!({
                "input": cable("in", 0),
                "sections": [{ "type": "lowShelf", "freq": cable("freq", 0), "gain": 12.0 }],
            }),
        ),
    ])
    .unwrap();
    assert_eq!(patch.sampleables["eq"].port_lanes("output"), 2);
    let mut out = Vec::new();
    for _ in 0..48 {
        out = step(&patch, "eq");
    }
    assert_ne!(out[0], out[1], "{out:?}");
}

#[test]
fn a_lane_sum_sums_each_side_on_its_own() {
    // Two stereo lanes into `$wowFlutter`'s speed: the module is stereo and
    // a mono input passes through both sides.
    let patch = build(&[
        ("in", "$signal", signal(json!(1.0))),
        ("a", "$signal", signal(json!([1.0, 2.0]))),
        (
            "speed",
            "$stereo",
            json!({ "left": [cable("a", 0), cable("a", 1)], "right": [cable("a", 1), cable("a", 0)] }),
        ),
        (
            "wf",
            "$wowFlutter",
            json!({ "input": cable("in", 0), "speed": [cable("speed", 0), cable("speed", 1)] }),
        ),
    ])
    .unwrap();
    assert_eq!(patch.sampleables["wf"].port_lanes("output"), 2);
}

#[test]
fn unison_copies_each_side_of_a_stereo_lane() {
    let patch = build(&[
        ("l", "$signal", signal(json!(0.0))),
        ("r", "$signal", signal(json!(1.0))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("l", 0), "right": cable("r", 0) }),
        ),
        (
            "u",
            "$unison",
            json!({ "input": cable("st", 0), "count": 2 }),
        ),
    ])
    .unwrap();
    assert_eq!(step(&patch, "u"), [0.0, 1.0, 0.0, 1.0]);
}

#[test]
fn spread_places_a_stereo_lane_by_its_logical_position() {
    let patch = build(&[
        ("lo", "$signal", signal(json!(0.0))),
        ("hi", "$signal", signal(json!(1.0))),
        (
            "min",
            "$stereo",
            json!({ "left": cable("lo", 0), "right": cable("hi", 0) }),
        ),
        (
            "s",
            "$spread",
            json!({ "min": cable("min", 0), "max": 4.0, "count": 2 }),
        ),
    ])
    .unwrap();
    assert_eq!(step(&patch, "s"), [0.0, 1.0, 4.0, 4.0]);
}

#[test]
fn quantizer_snaps_each_side() {
    let patch = build(&[
        ("l", "$signal", signal(json!(0.0))),
        ("r", "$signal", signal(json!(1.0))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("l", 0), "right": cable("r", 0) }),
        ),
        ("q", "$quantizer", json!({ "input": cable("st", 0) })),
    ])
    .unwrap();
    assert_eq!(step(&patch, "q"), [0.0, 1.0]);
}

/// The inputs a stereo or file-derived module reads side by side. Every
/// other input of such a module must be marked `#[input(…, fold)]`, so the
/// editor can show where a stereo signal folds.
const PER_SIDE: &[(&str, &[&str])] = &[
    ("$pan", &["input"]),
    ("$supersaw", &["freq", "detune", "fm", "sync"]),
];

#[test]
fn every_input_of_a_stereo_module_folds_or_reads_by_side() {
    use modular_core::types::StereoMode;
    for module in schema() {
        if !matches!(module.stereo, StereoMode::Stereo | StereoMode::Derived) {
            continue;
        }
        let per_side = PER_SIDE
            .iter()
            .find(|(name, _)| *name == module.name)
            .map_or(&[][..], |(_, inputs)| inputs);
        for param in &module.signal_params {
            assert!(
                param.fold != per_side.contains(&param.name.as_str()),
                "{}.{}: mark it `fold`, or list it in PER_SIDE",
                module.name,
                param.name
            );
        }
        let nested = nested_signal_params(&module);
        for name in &nested {
            assert!(
                module.nested_folds.contains(name) != per_side.contains(&name.as_str()),
                "{}.{name}: mark it `fold`, or list it in PER_SIDE",
                module.name,
            );
        }
        for name in &module.nested_folds {
            assert!(
                nested.contains(name),
                "{}.{name} is marked `fold` but holds no signal",
                module.name,
            );
        }
    }
}

/// The params of `module` that nest signals inside arrays, tuples or
/// structs, rather than being a signal themselves.
fn nested_signal_params(module: &modular_core::types::ModuleSchema) -> Vec<String> {
    fn holds_signal(node: &Value, defs: &Value, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        match node {
            Value::Object(map) => {
                if let Some(Value::String(reference)) = map.get("$ref") {
                    let name = reference.trim_start_matches("#/$defs/");
                    return matches!(name, "PolySignal" | "LaneSum" | "Signal")
                        || holds_signal(&defs[name], defs, depth + 1);
                }
                map.values().any(|v| holds_signal(v, defs, depth + 1))
            }
            Value::Array(items) => items.iter().any(|v| holds_signal(v, defs, depth + 1)),
            _ => false,
        }
    }
    let schema = serde_json::to_value(&module.params_schema.schema).unwrap();
    let defs = schema.get("$defs").cloned().unwrap_or(Value::Null);
    let Some(Value::Object(properties)) = schema.get("properties") else {
        return Vec::new();
    };
    properties
        .iter()
        .filter(|(name, _)| !module.signal_params.iter().any(|p| &p.name == *name))
        .filter(|(_, property)| holds_signal(property, &defs, 0))
        .map(|(name, _)| name.clone())
        .collect()
}

fn buffer_ref(module: &str) -> Value {
    json!({ "type": "buffer_ref", "module": module, "port": "buffer", "lanes": 1 })
}

#[test]
fn a_stereo_buffer_records_and_plays_each_side() {
    let patch = build(&[
        ("l", "$signal", signal(json!(1.0))),
        ("r", "$signal", signal(json!(2.0))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("l", 0), "right": cable("r", 0) }),
        ),
        (
            "buf",
            "$buffer",
            json!({ "input": cable("st", 0), "length": 0.01 }),
        ),
        (
            "tap",
            "$delayRead",
            json!({ "buffer": buffer_ref("buf"), "time": 0.001 }),
        ),
        (
            "scrub",
            "$bufRead",
            json!({ "buffer": buffer_ref("buf"), "frame": 0.0 }),
        ),
    ])
    .unwrap();
    let mut tap = Vec::new();
    for _ in 0..200 {
        tap = step(&patch, "tap");
    }
    assert_eq!(tap, [1.0, 2.0]);
    assert_eq!(step(&patch, "scrub"), [1.0, 2.0]);
}

#[test]
fn a_mono_buffer_feeds_both_sides_of_a_stereo_reader() {
    let patch = build(&[
        ("x", "$signal", signal(json!(3.0))),
        (
            "buf",
            "$buffer",
            json!({ "input": cable("x", 0), "length": 0.01 }),
        ),
        ("l", "$signal", signal(json!(0.001))),
        ("r", "$signal", signal(json!(0.002))),
        (
            "time",
            "$stereo",
            json!({ "left": cable("l", 0), "right": cable("r", 0) }),
        ),
        (
            "tap",
            "$delayRead",
            json!({ "buffer": buffer_ref("buf"), "time": cable("time", 0) }),
        ),
    ])
    .unwrap();
    let mut tap = Vec::new();
    for _ in 0..200 {
        tap = step(&patch, "tap");
    }
    assert_eq!(tap, [3.0, 3.0]);
}

/// Carry `old`'s state into `new`, built by [`insert`], and connect it, as a
/// patch update does.
fn swap(old: &Patch, new: &Patch) {
    for (id, module) in &new.sampleables {
        if let Some(previous) = old.sampleables.get(id) {
            module.transfer_state_from(previous.as_ref(), &SameIds);
        }
    }
    new.connect_all();
}

#[test]
fn a_coin_flip_widened_to_stereo_flips_both_sides_together() {
    let clock = json!({ "freq": 6.0 });
    let mono = build(&[
        ("clk", "$pulse", clock.clone()),
        (
            "flip",
            "$coinFlip",
            json!({ "input": cable("clk", 0), "probability": 2.5 }),
        ),
    ])
    .unwrap();
    for _ in 0..1000 {
        step(&mono, "flip");
    }
    let (stereo, _) = insert(&[
        ("clk", "$pulse", clock),
        ("half", "$signal", signal(json!(2.5))),
        (
            "both",
            "$stereo",
            json!({ "left": cable("half", 0), "right": cable("half", 0) }),
        ),
        (
            "flip",
            "$coinFlip",
            json!({ "input": cable("clk", 0), "probability": cable("both", 0) }),
        ),
    ])
    .unwrap();
    swap(&mono, &stereo);
    for _ in 0..8000 {
        let heads = step(&stereo, "flip");
        assert_eq!(heads[0], heads[1], "both sides flip alike");
    }
}

#[test]
fn a_buffer_widened_to_stereo_keeps_its_recording_on_both_sides() {
    let mono = build(&[
        ("one", "$signal", signal(json!(1.0))),
        (
            "buf",
            "$buffer",
            json!({ "input": cable("one", 0), "length": 0.01 }),
        ),
        (
            "tap",
            "$delayRead",
            json!({ "buffer": buffer_ref("buf"), "time": 0.005 }),
        ),
    ])
    .unwrap();
    for _ in 0..1000 {
        step(&mono, "tap");
    }
    let (stereo, _) = insert(&[
        ("zero", "$signal", signal(json!(0.0))),
        (
            "st",
            "$stereo",
            json!({ "left": cable("zero", 0), "right": cable("zero", 0) }),
        ),
        (
            "buf",
            "$buffer",
            json!({ "input": cable("st", 0), "length": 0.01 }),
        ),
        (
            "tap",
            "$delayRead",
            json!({ "buffer": buffer_ref("buf"), "time": 0.005 }),
        ),
    ])
    .unwrap();
    swap(&mono, &stereo);
    assert_eq!(step(&stereo, "tap"), [1.0, 1.0]);
}

#[test]
fn a_wow_flutter_widened_to_stereo_wobbles_both_sides_together() {
    let mono = build(&[
        ("osc", "$sine", json!({ "freq": 0.0 })),
        ("wow", "$wowFlutter", json!({ "input": cable("osc", 0) })),
    ])
    .unwrap();
    for _ in 0..1000 {
        step(&mono, "wow");
    }
    let (stereo, _) = insert(&[
        ("osc", "$sine", json!({ "freq": 0.0 })),
        (
            "st",
            "$stereo",
            json!({ "left": cable("osc", 0), "right": cable("osc", 0) }),
        ),
        ("wow", "$wowFlutter", json!({ "input": cable("st", 0) })),
    ])
    .unwrap();
    swap(&mono, &stereo);
    // The right side's delay line is new, so compare once it has filled.
    for i in 0..6000 {
        let out = step(&stereo, "wow");
        if i > 4000 {
            assert!(
                (out[0] - out[1]).abs() < 1e-4,
                "sides apart at {i}: {out:?}"
            );
        }
    }
}

#[test]
fn pan_has_a_lane_for_each_input_lane() {
    let patch = build(&[
        ("osc", "$signal", signal(json!(1.0))),
        (
            "panned",
            "$pan",
            json!({ "input": cable("osc", 0), "pan": [-5.0, 0.0, 5.0] }),
        ),
    ])
    .unwrap();
    // One stereo lane, the input placed hard left by the first pan lane.
    let out = step(&patch, "panned");
    assert_eq!(out.len(), 2);
    assert!(out[1].abs() < 1e-6, "{out:?}");
}

fn wav(channels: usize) -> Value {
    json!({ "type": "wav_ref", "path": format!("file{channels}"), "channels": channels })
}

#[test]
fn a_two_channel_file_makes_its_players_stereo() {
    let patch = build(&[
        ("stereo", "$sampler", json!({ "wav": wav(2), "gate": 5.0 })),
        ("mono", "$sampler", json!({ "wav": wav(1), "gate": 5.0 })),
        ("quad", "$sampler", json!({ "wav": wav(4), "gate": 5.0 })),
        (
            "cloud",
            "$grains",
            json!({ "pitch": [0.0, 1.0], "wav": wav(2), "gate": 5.0 }),
        ),
        (
            "filtered",
            "$add",
            json!({ "input": cable("stereo", 0), "offset": 0.0 }),
        ),
    ])
    .unwrap();
    let lanes = |id: &str| patch.sampleables[id].port_lanes("output");
    // One stereo lane, one mono lane, and a mono lane per channel.
    assert_eq!(lanes("stereo"), 2);
    assert_eq!(lanes("mono"), 1);
    assert_eq!(lanes("quad"), 4);
    // Each voice of `$grains` is a stereo lane.
    assert_eq!(lanes("cloud"), 4);
    // What reads a stereo player lifts to stereo.
    assert_eq!(lanes("filtered"), 2);
}

#[test]
fn a_stereo_file_is_found_inside_containers() {
    use modular_core::types::{Connect, Wav};
    let found = |value: &dyn Fn(&mut bool)| {
        let mut found = false;
        value(&mut found);
        found
    };
    let stereo = || Wav::new("stereo".into(), 2);
    let mono = || Wav::new("mono".into(), 1);
    assert!(found(&|f| Some(stereo()).find_stereo_file(f)));
    assert!(!found(&|f| Some(mono()).find_stereo_file(f)));
    assert!(!found(&|f| None::<Wav>.find_stereo_file(f)));
    assert!(found(&|f| vec![mono(), stereo()].find_stereo_file(f)));
    assert!(found(&|f| Box::new((1.0_f32, stereo())).find_stereo_file(f)));
}

/// Render `samples` samples of a patch whose `$grains` "cloud" granulates a
/// two-channel file, left at 0.4 and right at -0.2, and return the cloud's
/// physical lanes at the last one.
fn render_stereo_cloud(modules: &[(&str, &str, Value)], samples: usize) -> Vec<f32> {
    use modular_core::types::{SampleBuffer, WavData};
    let (mut patch, _) = insert(modules).unwrap();
    patch.wav_data.insert(
        "file2".into(),
        std::sync::Arc::new(WavData::new(
            SampleBuffer::from_samples(vec![vec![0.4; 48000], vec![-0.2; 48000]], SAMPLE_RATE),
            None,
        )),
    );
    patch.connect_all();
    let mut out = Vec::new();
    for _ in 0..samples {
        out = step(&patch, "cloud");
    }
    out
}

fn cloud(pitch: Value, gate: Value) -> (&'static str, &'static str, Value) {
    (
        "cloud",
        "$grains",
        json!({
            "pitch": pitch,
            "wav": wav(2),
            "gate": gate,
            "density": 0.01,
            "loopCount": 1,
            "shape": "square",
            "length": 0.5,
        }),
    )
}

#[test]
fn grains_plays_each_side_of_a_stereo_file() {
    // Two voices, each a stereo lane: voice `v` side `c` at `v * 2 + c`.
    let out = render_stereo_cloud(&[cloud(json!([0.0, 1.0]), json!(5.0))], 100);
    assert_eq!(out.len(), 4);
    for voice in out.chunks(2) {
        assert!(voice[0] > 0.0, "{out:?}");
        assert!((voice[1] + 0.5 * voice[0]).abs() < 1e-5, "{out:?}");
    }
}

#[test]
fn grains_reads_its_inputs_folded_from_a_stereo_file() {
    // A gate high on one side only folds to 2.5 V, which spawns grains
    // whichever side it is on.
    for (left, right) in [(0.0, 5.0), (5.0, 0.0)] {
        let out = render_stereo_cloud(
            &[
                ("l", "$signal", signal(json!(left))),
                ("r", "$signal", signal(json!(right))),
                (
                    "st",
                    "$stereo",
                    json!({ "left": cable("l", 0), "right": cable("r", 0) }),
                ),
                cloud(json!(0.0), cable("st", 0)),
            ],
            100,
        );
        assert_eq!(out.len(), 2);
        assert!(out[0] > 0.0, "gate ({left}, {right}): {out:?}");
        assert!((out[1] + 0.5 * out[0]).abs() < 1e-5, "{out:?}");
    }
}
