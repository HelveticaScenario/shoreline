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

/// A patched trigger holds the accent from its strike: an accent that moves
/// mid-hit changes nothing, in the level or in the voice's own response.
#[test]
fn a_hit_keeps_the_accent_it_was_struck_with() {
    for (module, params, port) in struck() {
        let render = |accent_hz: f32| {
            render_clocks(
                module,
                params.clone(),
                &[(port, 1.0), ("accent", accent_hz)],
                output(module),
                0.9,
            )
            .0
        };
        assert_eq!(render(7.0), render(13.0), "{module}");
    }
}

/// Peaks of the hits struck at `strikes`, past the first and leaving out
/// every `skip`th, whose strike lands on an edge of the accent.
fn hit_peaks(x: &[f32], strikes: &[usize], skip: usize) -> Vec<f32> {
    strikes
        .windows(2)
        .enumerate()
        .filter(|(i, _)| i % skip != 0)
        .map(|(_, w)| peak(&x[w[0]..w[1]]))
        .collect()
}

/// Accented hits struck over ringing ones stand out about as far as hits
/// struck at a steady full accent stand out from steady unaccented ones.
#[test]
fn accents_land_on_overlapping_hits() {
    // Seven strikes a second against an accent that flips four times a
    // second, so the accent changes while hits ring.
    const STRIKES_HZ: f32 = 7.0;
    const ACCENT_HZ: f32 = 2.0;
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    for (module, params, port) in struck() {
        let mut params = params.clone();
        if !matches!(module, "$mi.sixOp" | "$mi.speak") {
            params["decay"] = json!(4.0);
        }
        let steady = |accent: f32| {
            let mut params = params.clone();
            params["accent"] = json!(accent);
            let (x, strikes) =
                render_clocks(module, params, &[(port, STRIKES_HZ)], output(module), 2.0);
            mean(&hit_peaks(&x, &strikes, 7))
        };
        let expected = steady(5.0) / steady(0.0);
        let (x, strikes) = render_clocks(
            module,
            params,
            &[(port, STRIKES_HZ), ("accent", ACCENT_HZ)],
            output(module),
            2.0,
        );
        let mut peaks = hit_peaks(&x, &strikes, 7);
        peaks.sort_by(f32::total_cmp);
        let third = peaks.len() / 3;
        let contrast = mean(&peaks[peaks.len() - third..]) / mean(&peaks[..third]);
        assert!(
            contrast >= 0.8 * expected,
            "{module}: {contrast} of {expected}"
        );
    }
}
