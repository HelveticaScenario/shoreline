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

#[test]
fn enosc_plays_odd_voices_left_and_even_voices_right() {
    let patch = build(&[
        ("e", "$enosc", json!({ "pitch": 0.0, "count": 4 })),
        (
            "odd",
            "$add",
            json!({
                "input": { "type": "cable", "module": "e", "port": "odd", "lane": 0 },
                "offset": 0.0,
            }),
        ),
    ])
    .unwrap();
    let mut heard = false;
    for _ in 0..512 {
        let stereo = step(&patch, "e");
        assert_eq!(stereo.len(), 2);
        let module = &patch.sampleables["e"];
        let [all, odd, even] = ["all", "odd", "even"].map(|port| module.get_value_at(port, 0, 0));
        assert!((stereo[0] - odd).abs() < 1e-5, "{stereo:?} {odd}");
        assert!((stereo[1] - even).abs() < 1e-5, "{stereo:?} {even}");
        assert!((stereo[0] + stereo[1] - all).abs() < 1e-3);
        heard |= stereo[0] != stereo[1];
    }
    assert!(heard, "the sides differ");
    // A split output is mono, so what reads it stays mono.
    assert_eq!(patch.sampleables["e"].port_lanes("odd"), 1);
    assert_eq!(step(&patch, "odd").len(), 1);
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
