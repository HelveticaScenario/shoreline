//! Filters must stay bounded when their frequency is modulated hard: a ±5 V
//! input may ring up to the filter's fixed-frequency resonant peak, never far
//! past it.

use serde_json::{Value, json};

mod common;
use common::{finite_peak, render_ports};

const SAMPLE_RATE: f32 = 48_000.0;
const FRAMES: usize = 24_000;

fn cable(module: &str) -> Value {
    json!({ "type": "cable", "module": module, "port": "output", "lane": 0 })
}

fn filter_params(module_type: &str, freq: Value) -> Value {
    match module_type {
        "$bpf" => json!({ "input": cable("src"), "center": freq, "resonance": 4.5 }),
        "$xover" => json!({ "input": cable("src"), "lowMidFreq": freq }),
        _ => json!({ "input": cable("src"), "cutoff": freq, "resonance": 4.5 }),
    }
}

/// Peak |output| over every output port of the filter.
fn peak(module_type: &str, freq: Value, modulator: Option<(&str, Value)>) -> f32 {
    let mut modules = vec![
        ("src", "$sine", json!({ "freq": -1.25 })),
        ("filt", module_type, filter_params(module_type, freq)),
    ];
    if let Some((t, p)) = modulator {
        modules.push(("mod", t, p));
    }
    render_ports(modules, SAMPLE_RATE, FRAMES, "filt")
        .iter()
        .map(|(_, samples)| finite_peak(samples))
        .fold(0.0, f32::max)
}

#[test]
fn filters_stay_bounded_under_frequency_modulation() {
    for module_type in ["$lpf", "$hpf", "$bpf", "$xover"] {
        let ceiling = [-5.0f32, -3.0, -1.0, 1.0, 3.0, 5.0]
            .into_iter()
            .map(|v| peak(module_type, json!(v), None))
            .fold(5.0f32, f32::max);
        for (name, modulator) in [
            ("stepped", ("$pulse", json!({ "freq": -2.0 }))),
            ("audio-rate", ("$sine", json!({ "freq": 0.0 }))),
        ] {
            let p = peak(module_type, cable("mod"), Some(modulator));
            assert!(
                p <= ceiling * 1.5,
                "{module_type} {name} modulation peaked at {p} V (fixed-frequency ceiling {ceiling} V)"
            );
        }
    }
}
