use super::*;
use crate::dsp::mi::core::fixture::{max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(structure: f32, brightness: f32, decay: f32, accent: f32) -> Knobs {
    Knobs {
        structure: (structure, mark()),
        brightness: (brightness, mark()),
        decay: (decay, mark()),
        accent: (accent, mark()),
    }
}

/// Replays `modal.f32` (Plaits' modal voice rendered a sample at a time):
/// strikes at several pitches, then grains with the trigger unpatched.
#[test]
fn matches_plaits() {
    let state = ModalState::new(48000.0);
    let mut voice = Voice::new(0x1234567);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("modal", 7 + 2) {
        let (body, exciter) = voice.step(
            record[0],
            knobs(record[1], record[2], record[3], record[4]),
            record[5] != 0.0,
            record[6] != 0.0,
            0.0,
            &state,
        );
        actual.extend([body, exciter]);
        expected.extend([record[7], record[8]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// A strike's spectrum: the level of each of the first eight partials, by
/// direct DFT at the struck pitch's multiples over the first 50 ms. A short
/// window, because Plaits' tangent approximation tunes upper modes a fraction
/// of a hertz differently at each rate, which a long window would read as a
/// level change.
fn strike_spectrum(sample_rate: f32, hz: f32, structure: f32, decay: f32) -> Vec<f32> {
    let state = ModalState::new(sample_rate);
    let mut voice = Voice::new(1);
    let f0 = hz / sample_rate;
    let length = (sample_rate * 0.05) as usize;
    let out: Vec<f32> = (0..length)
        .map(|n| {
            voice
                .step(
                    f0,
                    knobs(structure, 2.5, decay, 4.0),
                    false,
                    n == 0,
                    0.0,
                    &state,
                )
                .0
        })
        .collect();
    (1..=8)
        .map(|k| {
            let w = std::f64::consts::TAU * f64::from(hz) * k as f64 / f64::from(sample_rate);
            let (re, im) = out
                .iter()
                .enumerate()
                .fold((0.0f64, 0.0f64), |(re, im), (n, &x)| {
                    let phase = w * n as f64;
                    (
                        re + f64::from(x) * phase.cos(),
                        im + f64::from(x) * phase.sin(),
                    )
                });
            ((re * re + im * im).sqrt() / f64::from(sample_rate)) as f32
        })
        .collect()
}

/// A harmonic strike rings with the same partial levels at 44.1 and 96 kHz as
/// at 48 kHz.
#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, decay) in [(-1.0, 2.0), (0.5, 3.5)] {
        let hz = voct_to_hz(pitch);
        let reference = strike_spectrum(48000.0, hz, 1.4, decay);
        let loudest = reference.iter().fold(0.0f32, |m, &a| m.max(a));
        for sample_rate in [44100.0, 96000.0] {
            let actual = strike_spectrum(sample_rate, hz, 1.4, decay);
            for (k, (a, r)) in actual.iter().zip(&reference).enumerate() {
                assert!(
                    (a - r).abs() < 0.1 * loudest,
                    "{sample_rate} Hz, pitch {pitch}, partial {}: {a} vs {r}",
                    k + 1
                );
            }
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = ModalState::new(48000.0);
    let mut held = Voice::new(7);
    let mut glitched = Voice::new(7);
    for n in 0..2000 {
        let v = match n {
            500 => f32::NAN,
            501 => f32::INFINITY,
            502 => f32::NEG_INFINITY,
            _ => 2.0,
        };
        let strike = n % 400 == 0;
        let a = held
            .step(0.01, knobs(2.0, 2.0, 2.0, 2.0), false, strike, 0.0, &state)
            .0;
        let b = glitched
            .step(0.01, knobs(v, v, v, v), false, strike, 0.0, &state)
            .0;
        assert!((a - b).abs() < 1e-6, "sample {n}: {a} vs {b}");
    }
}

/// Extreme pitches, knob jumps, dense grains, a full-scale input and any
/// engine rate leave the output finite and bounded.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 44100.0, 96000.0] {
        let state = ModalState::new(sample_rate);
        let mut voice = Voice::new(3);
        let mut peak = 0.0f32;
        for n in 0..20000 {
            let f0 = match n % 5 {
                0 => 0.5,
                1 => 0.0,
                _ => 0.002 + (n as f32 * 0.001).sin().abs() * 0.05,
            };
            let knob = (n % 11) as f32 * 0.5;
            let input = if n % 3 == 0 { 1.0 } else { -1.0 };
            let (body, exciter) = voice.step(
                f0,
                knobs(knob, 5.0 - knob, knob, 5.0),
                n % 2000 < 1000,
                n % 97 == 0,
                input,
                &state,
            );
            assert!(
                body.is_finite() && exciter.is_finite(),
                "{sample_rate} Hz, sample {n}"
            );
            peak = peak.max(body.abs());
        }
        // Strikes and a full-scale input land before the limiter's attack
        // pulls the gain down, so the first transients pass 1; nothing grows.
        assert!(peak < 8.0, "{sample_rate} Hz: peak {peak}");
    }
}
