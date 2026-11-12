//! `accent` on the Plaits voices: 0 V plays at Plaits' default accent, 5 V
//! hits twice as hard, and the level a strike takes holds through the hit
//! without stepping what still rings.

mod drum_common;

use drum_common::*;
use serde_json::{Value, json};

/// Each voice with the params that strike it once at the start, and the port
/// they strike through.
fn struck() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        ("$mi.kick", json!({ "freq": voct(50.0) }), "trigger"),
        ("$mi.synthKick", json!({ "freq": voct(50.0) }), "trigger"),
        ("$mi.snare", json!({ "freq": voct(200.0) }), "trigger"),
        ("$mi.synthSnare", json!({ "freq": voct(200.0) }), "trigger"),
        ("$mi.hiHat", json!({ "freq": voct(300.0) }), "trigger"),
        ("$mi.ringHat", json!({ "freq": voct(300.0) }), "trigger"),
        ("$mi.modal", json!({ "freq": voct(220.0) }), "trigger"),
        ("$mi.string", json!({ "freq": voct(220.0) }), "trigger"),
        ("$mi.sixOp", json!({ "freq": voct(220.0) }), "gate"),
        (
            "$mi.speak",
            json!({ "freq": voct(120.0), "word": 1.0 }),
            "trigger",
        ),
    ]
}

fn output(module: &str) -> &'static str {
    match module {
        "$mi.modal" => "body",
        "$mi.string" => "strings",
        _ => "output",
    }
}

/// Half a second of `module` struck through `port`, with `accent` if given.
fn hit(module: &str, mut params: Value, port: &str, accent: Option<f32>) -> Vec<f32> {
    params[port] = json!(5.0);
    if let Some(accent) = accent {
        params["accent"] = json!(accent);
    }
    render_from(module, params, output(module), 0.5, SR)
}

#[test]
fn full_accent_doubles_the_level() {
    for (module, params, port) in struck() {
        let soft = peak(&hit(module, params.clone(), port, Some(0.0)));
        let hard = peak(&hit(module, params, port, Some(5.0)));
        let ratio = hard / soft;
        assert!((1.75..=2.25).contains(&ratio), "{module}: {soft} to {hard}");
    }
}

#[test]
fn no_accent_is_the_default() {
    for (module, params, port) in struck() {
        let unaccented = hit(module, params.clone(), port, Some(0.0));
        assert_eq!(unaccented, hit(module, params, port, None), "{module}");
    }
}

/// Unpatched, a drum's drone grows with the accent.
#[test]
fn accent_raises_a_drone() {
    for module in ["$mi.kick", "$mi.snare", "$mi.hiHat", "$mi.modal"] {
        let drone = |accent: f32| {
            let params = json!({ "freq": voct(110.0), "accent": accent });
            let x = render_from(module, params, output(module), 1.0, SR);
            rms(&x[x.len() / 2..])
        };
        let (soft, hard) = (drone(0.0), drone(5.0));
        assert!(soft > 0.0, "{module} silent unaccented");
        assert!(hard > 1.5 * soft, "{module}: {soft} to {hard}");
    }
}

/// The biggest sample-to-sample change in the first few samples of each
/// strike after the first.
fn strike_steps(x: &[f32], strikes: &[usize]) -> Vec<f32> {
    strikes[1..]
        .iter()
        .map(|&s| {
            (s..s + 4)
                .map(|n| (x[n] - x[n - 1]).abs())
                .fold(0.0, f32::max)
        })
        .collect()
}

/// A step in volts too small to hear.
const INAUDIBLE: f32 = 5.0e-3;

/// A strike that changes the accent while the last hit still rings steps
/// the output no more than a fully accented strike does.
#[test]
fn changing_accent_does_not_step_a_ringing_voice() {
    for (module, params, port) in struck() {
        let mut full = params.clone();
        full["accent"] = json!(5.0);
        let (x, strikes) = render_clocks(module, full, &[(port, 4.0)], output(module), 2.0);
        let bound = strike_steps(&x, &strikes)
            .into_iter()
            .fold(0.0f32, f32::max);
        let (x, strikes) = render_clocks(
            module,
            params,
            &[(port, 4.0), ("accent", 2.0)],
            output(module),
            2.0,
        );
        for step in strike_steps(&x, &strikes) {
            assert!(
                step <= 1.05 * bound + INAUDIBLE,
                "{module}: {step} past {bound}"
            );
        }
    }
}
