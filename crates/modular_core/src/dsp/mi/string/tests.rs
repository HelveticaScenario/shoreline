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

fn state(sample_rate: f32) -> MiStringState {
    MiStringState {
        sample_rate,
        inv_sample_rate: 1.0 / sample_rate,
        rates: StringRates::new(sample_rate),
        limiter: LimiterRates::new(sample_rate),
    }
}

/// Replays `string.f32` (Plaits' three strings rendered a sample at a time):
/// plucks from below the delay line's reach up the keyboard across the
/// structure range, then grains with the trigger unpatched.
#[test]
fn matches_plaits() {
    let state = state(48000.0);
    let mut voice = Voice::new(0x1234567, &state.rates);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("string", 7 + 2) {
        let (strings, exciter) = voice.step(
            record[0],
            knobs(record[1], record[2], record[3], record[4]),
            record[5] != 0.0,
            record[6] != 0.0,
            0.0,
            &state,
        );
        actual.extend([strings, exciter]);
        expected.extend([record[7], record[8]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// The string's first eight partial levels over its first 50 ms, rung through
/// `input` by a 5 ms windowed sine at its pitch: the same excitation at any
/// rate, which a pluck's random noise burst is not.
fn ring_spectrum(sample_rate: f32, hz: f32, structure: f32) -> Vec<f32> {
    let state = state(sample_rate);
    let mut voice = Voice::new(1, &state.rates);
    let f0 = hz / sample_rate;
    let burst = (sample_rate * 0.005) as usize;
    let out: Vec<f32> = (0..(sample_rate * 0.05) as usize)
        .map(|n| {
            let input = if n < burst {
                let t = n as f32 / burst as f32;
                let window = 0.5 - 0.5 * (std::f32::consts::TAU * t).cos();
                window * (std::f32::consts::TAU * hz * n as f32 / sample_rate).sin()
            } else {
                0.0
            };
            voice
                .step(
                    f0,
                    knobs(structure, 2.5, 3.0, 4.0),
                    false,
                    false,
                    input,
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

/// A clean string rings with the same partial levels at 44.1 and 96 kHz as
/// at 48 kHz.
#[test]
fn sounds_the_same_at_any_rate() {
    for pitch in [-1.0, 0.5] {
        let hz = voct_to_hz(pitch);
        let reference = ring_spectrum(48000.0, hz, 1.25);
        let loudest = reference.iter().fold(0.0f32, |m, &a| m.max(a));
        for sample_rate in [44100.0, 96000.0] {
            let actual = ring_spectrum(sample_rate, hz, 1.25);
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
    let state = state(48000.0);
    let mut held = Voice::new(7, &state.rates);
    let mut glitched = Voice::new(7, &state.rates);
    for n in 0..4000 {
        let v = match n {
            500 => f32::NAN,
            501 => f32::INFINITY,
            502 => f32::NEG_INFINITY,
            _ => 2.0,
        };
        let pluck = n % 400 == 0;
        let a = held
            .step(0.01, knobs(2.0, 2.0, 2.0, 2.0), false, pluck, 0.0, &state)
            .0;
        let b = glitched
            .step(0.01, knobs(v, v, v, v), false, pluck, 0.0, &state)
            .0;
        assert!((a - b).abs() < 1e-6, "sample {n}: {a} vs {b}");
    }
}

/// Extreme pitches, knob jumps, dense grains, a full-scale input and any
/// engine rate leave the output finite and bounded.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 44100.0, 96000.0] {
        let state = state(sample_rate);
        let mut voice = Voice::new(3, &state.rates);
        let mut peak = 0.0f32;
        for n in 0..20000 {
            let f0 = match n % 5 {
                0 => 0.5,
                1 => 1.0 / sample_rate,
                _ => 0.0005 + (n as f32 * 0.001).sin().abs() * 0.05,
            };
            let knob = (n % 11) as f32 * 0.5;
            let input = if n % 3 == 0 { 1.0 } else { -1.0 };
            let (strings, exciter) = voice.step(
                f0,
                knobs(knob, 5.0 - knob, knob, 5.0),
                n % 4000 < 2000,
                n % 97 == 0,
                input,
                &state,
            );
            assert!(
                strings.is_finite() && exciter.is_finite(),
                "{sample_rate} Hz, sample {n}"
            );
            peak = peak.max(strings.abs());
        }
        // Plucks and a full-scale input land before the limiter's attack pulls
        // the gain down, so the first transients pass 1; nothing grows.
        assert!(peak < 8.0, "{sample_rate} Hz: peak {peak}");
    }
}
