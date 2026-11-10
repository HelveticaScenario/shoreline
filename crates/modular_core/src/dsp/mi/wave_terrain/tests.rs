use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(terrain: f32, radius: f32, offset: f32) -> Knobs {
    Knobs {
        terrain: (terrain, mark()),
        radius: (radius, mark()),
        offset: (offset, mark()),
    }
}

fn render(voice: &mut Voice, f0: f32, k: Knobs, state: &WaveTerrainState) -> (f32, f32) {
    let controls = voice.control(f0, k, state);
    voice.step(controls)
}

/// `seconds` of a tone at `hz` (negative runs backward) and fixed knobs, one
/// output.
fn tone(sample_rate: f32, hz: f32, settings: (f32, f32, f32), pm: bool, seconds: f32) -> Vec<f32> {
    let state = WaveTerrainState::new(sample_rate);
    let mut voice = Voice::default();
    (0..(sample_rate * seconds) as usize)
        .map(|_| {
            let (out, aux) = render(
                &mut voice,
                hz / sample_rate,
                knobs(settings.0, settings.1, settings.2),
                &state,
            );
            if pm { aux } else { out }
        })
        .collect()
}

/// Replays `wave_terrain.f32` (Plaits' path oscillator and terrains,
/// rendered a sample at a time): eight pitches, each from a fresh voice, with
/// the knobs sweeping all eight terrains.
#[test]
fn matches_plaits() {
    let state = WaveTerrainState::new(48000.0);
    let mut voice = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (n, record) in records("wave_terrain", 4 + 2).iter().enumerate() {
        if n % 2400 == 0 {
            voice = Voice::default();
        }
        let (out, pm) = render(
            &mut voice,
            record[0],
            knobs(record[1], record[2], record[3]),
            &state,
        );
        actual.extend([out, pm]);
        expected.extend([record[4], record[5]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// A steady tone has the same harmonics at 44.1 and 96 kHz as at 48 kHz.
/// Plaits' path oscillator traces an ellipse whose skew grows with its step
/// per sample, so the tone drifts with the rate by about 1% per octave above
/// c4; these pitches stay within 2%.
#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, settings) in [
        (-2.0, (0.5, 2.5, 2.5)),
        (-1.0, (3.0, 4.0, 1.0)),
        (0.0, (4.8, 3.0, 4.0)),
    ] {
        for pm in [false, true] {
            assert_rate_independent(
                |sample_rate| tone(sample_rate, voct_to_hz(pitch), settings, pm, 0.5),
                voct_to_hz(pitch),
                0.02,
            );
        }
    }
}

/// A pitch running backward traces the circle the other way: the same
/// harmonics as forward.
#[test]
fn runs_backward() {
    // A whole number of cycles in the measured window, so no harmonic leaks
    // into another.
    let hz = 200.0;
    for settings in [(1.0, 3.0, 2.0), (4.0, 4.0, 3.5)] {
        for pm in [false, true] {
            let forward = harmonics(&tone(48000.0, hz, settings, pm, 0.5), hz, 48000.0, 16);
            let backward = harmonics(&tone(48000.0, -hz, settings, pm, 0.5), hz, 48000.0, 16);
            let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
            for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
                assert!(
                    (b - f).abs() < 0.01 * loudest,
                    "{settings:?} pm {pm}, harmonic {}: {b} vs {f}",
                    h + 1
                );
            }
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = WaveTerrainState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..300 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 1.0,
        };
        let a = render(&mut held, 0.01, knobs(1.0, 1.0, 1.0), &state);
        let b = render(&mut glitched, 0.01, knobs(v, v, v), &state);
        assert_eq!(a, b, "sample {n}");
    }
}

/// Every knob setting, any pitch including NaN and infinite ones, at any
/// rate, leaves both outputs finite and within Plaits' full scale.
#[test]
fn stays_finite_and_bounded() {
    for sample_rate in [8000.0, 44100.0, 96000.0] {
        let state = WaveTerrainState::new(sample_rate);
        let mut voice = Voice::default();
        for n in 0..40000 {
            let f0 = match n % 500 {
                100 => f32::NAN,
                150 => f32::INFINITY,
                200 => f32::NEG_INFINITY,
                _ => (n % 11) as f32 * 0.05 - 0.25,
            };
            let t = n as f32 * 0.002;
            let k = knobs(
                2.5 + 3.0 * t.sin(),
                2.5 + 3.0 * (t * 1.3).sin(),
                2.5 + 3.0 * (t * 0.7).sin(),
            );
            let (out, pm) = render(&mut voice, f0, k, &state);
            assert!(
                out.is_finite() && pm.is_finite() && out.abs() <= 1.5 && pm.abs() <= 1.0,
                "{sample_rate} Hz, sample {n}: {out}, {pm}"
            );
        }
    }
}
