//! The stereo effects take and give one stereo lane: each side of the input,
//! summed over its lanes, feeds that side of the effect.

use std::collections::HashMap;

use modular_core::types::{ModuleSpec, PatchGraph};
use serde_json::{Value, json};

mod common;
use common::from_graph;

const SAMPLE_RATE: f32 = 48_000.0;
const FRAMES: usize = 20_000;

const EFFECTS: &[&str] = &[
    "$dattorro",
    "$plate",
    "$unstable.reverb",
    "$unstable.reverb2",
    "$reverb",
    "$mi.chorus",
];

/// The effects whose tank keeps each side of the input apart.
const SIDED: &[&str] = &[
    "$unstable.reverb",
    "$unstable.reverb2",
    "$reverb",
    "$mi.chorus",
];

fn cable(module: &str, lane: usize) -> Value {
    json!({ "type": "cable", "module": module, "port": "output", "lane": lane })
}

/// The effect `fx`'s left and right sides over `FRAMES` samples, fed by
/// `modules`.
fn render(modules: &[(&str, &str, Value)]) -> (Vec<f32>, Vec<f32>) {
    let graph = PatchGraph {
        modules: modules
            .iter()
            .map(|(id, module_type, params)| ModuleSpec {
                id: id.to_string(),
                module_type: module_type.to_string(),
                id_is_explicit: None,
                skip_state_transfer: None,
                params: params.clone(),
            })
            .collect(),
        module_id_remaps: None,
        scopes: vec![],
        vu_meters: vec![],
    };
    let patch = from_graph(&graph, SAMPLE_RATE, 1, &HashMap::new()).unwrap();
    let fx = &patch.sampleables["fx"];
    assert_eq!(fx.port_lanes("output"), 2, "one stereo lane");
    let (mut left, mut right) = (Vec::with_capacity(FRAMES), Vec::with_capacity(FRAMES));
    for _ in 0..FRAMES {
        for module in patch.sampleables.values() {
            module.start_block();
        }
        for module in patch.sampleables.values() {
            module.ensure_processed();
        }
        left.push(fx.get_value_at("output", 0, 0));
        right.push(fx.get_value_at("output", 1, 0));
    }
    (left, right)
}

fn energy(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s * s).sum()
}

/// `effect` fed by `$stereo(sine, 0)`, with `params` beside its input.
fn hard_left(effect: &str, params: Value) -> (Vec<f32>, Vec<f32>) {
    let mut fx = params;
    fx["input"] = cable("st", 0);
    render(&[
        ("src", "$sine", json!({ "freq": 0.0 })),
        (
            "st",
            "$stereo",
            json!({ "left": cable("src", 0), "right": 0.0 }),
        ),
        ("fx", effect, fx),
    ])
}

#[test]
fn a_hard_left_input_reaches_the_left_side() {
    for &effect in EFFECTS {
        let (left, right) = hard_left(effect, json!({}));
        let (left, right) = (energy(&left), energy(&right));
        assert!(left > 0.0, "{effect}: the left side hears the input");
        if SIDED.contains(&effect) {
            assert!(
                left > right * 1.3,
                "{effect}: a hard-left input stays left: L={left}, R={right}"
            );
        }
    }
}

#[test]
fn a_wider_reverb_keeps_more_of_a_panned_input_on_its_side() {
    for effect in ["$unstable.reverb", "$unstable.reverb2"] {
        let bias = |width: f32| {
            let (left, right) = hard_left(effect, json!({ "decay": 2.0, "width": width }));
            energy(&left) / energy(&right).max(1e-9)
        };
        let (wide, narrow) = (bias(5.0), bias(-5.0));
        assert!(wide > narrow, "{effect}: wide={wide}, narrow={narrow}");
    }
}

/// A chord sounds through an effect as its voices summed, each at its own
/// level.
#[test]
fn every_lane_of_a_poly_input_is_summed_at_its_own_level() {
    let src = ("src", "$sine", json!({ "freq": [0.0, 0.58] }));
    for &effect in EFFECTS {
        let (chord_left, chord_right) = render(&[
            src.clone(),
            (
                "fx",
                effect,
                json!({ "input": [cable("src", 0), cable("src", 1)] }),
            ),
        ]);
        let (sum_left, sum_right) = render(&[
            src.clone(),
            (
                "sum",
                "$mix",
                json!({ "inputs": [cable("src", 0), cable("src", 1)] }),
            ),
            ("fx", effect, json!({ "input": cable("sum", 0) })),
        ]);
        for (chord, sum) in [(chord_left, sum_left), (chord_right, sum_right)] {
            let worst = chord
                .iter()
                .zip(&sum)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(
                worst < 1e-4,
                "{effect}: chord and summed chord differ by {worst}"
            );
        }
    }
}
