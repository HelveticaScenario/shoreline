use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

/// `seconds` of a tone at `hz` (negative runs backward) at a fixed
/// registration.
fn tone(sample_rate: f32, hz: f32, registration: f32, seconds: f32) -> Vec<f32> {
    let state = DivideDownState::new(sample_rate);
    let mut voice = Voice::default();
    (0..(sample_rate * seconds) as usize)
        .map(|_| voice.step(hz / sample_rate, (registration, mark()), &state))
        .collect()
}

/// Replays `divide_down.f32` (Plaits' divide-down oscillator and string
/// machine registrations, rendered a sample at a time): pitch steps, a sweep
/// and a pitch high enough to shift octaves, the registration sweeping.
#[test]
fn matches_plaits() {
    let state = DivideDownState::new(48000.0);
    let mut voice = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("divide_down", 2 + 1) {
        actual.push(voice.step(record[0], (record[1], mark()), &state));
        expected.push(record[2]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {at}");
}

/// A steady tone has the same harmonics at 44.1 and 96 kHz as at 48 kHz.
#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, registration) in [(-1.0, 0.0), (0.0, 2.5), (1.0, 5.0), (-2.0, 3.7)] {
        assert_rate_independent(
            |sample_rate| tone(sample_rate, voct_to_hz(pitch), registration, 0.5),
            voct_to_hz(pitch),
            0.02,
        );
    }
}

/// A pitch running backward plays the same harmonics as forward.
#[test]
fn runs_backward() {
    // A whole number of cycles in the measured window, so no harmonic leaks
    // into another.
    let hz = 200.0;
    for registration in [0.0, 1.3, 2.5, 4.1, 5.0] {
        let forward = harmonics(&tone(48000.0, hz, registration, 0.5), hz, 48000.0, 16);
        let backward = harmonics(&tone(48000.0, -hz, registration, 0.5), hz, 48000.0, 16);
        let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
        for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
            assert!(
                (b - f).abs() < 0.01 * loudest,
                "registration {registration}, harmonic {}: {b} vs {f}",
                h + 1
            );
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = DivideDownState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..300 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 1.0,
        };
        let a = held.step(0.01, (1.0, mark()), &state);
        let b = glitched.step(0.01, (v, mark()), &state);
        assert_eq!(a, b, "sample {n}");
    }
}

/// Every registration and any pitch, including NaN, infinite and backward
/// ones, at any rate, leave the output finite and within full scale.
#[test]
fn stays_finite_and_bounded() {
    for sample_rate in [8000.0, 44100.0, 96000.0] {
        let state = DivideDownState::new(sample_rate);
        let mut voice = Voice::default();
        for n in 0..40000 {
            let f0 = match n % 500 {
                100 => f32::NAN,
                150 => f32::INFINITY,
                200 => f32::NEG_INFINITY,
                _ => (n % 11) as f32 * 0.05 - 0.25,
            };
            let registration = 2.5 + 3.0 * (n as f32 * 0.002).sin();
            let out = voice.step(f0, (registration, mark()), &state);
            assert!(
                out.is_finite() && out.abs() <= 1.05,
                "{sample_rate} Hz, sample {n}: {out}"
            );
        }
    }
}

/// Audio-rate FM that swings the pitch across the octave shift keeps the
/// output within full scale at any registration.
#[test]
fn fm_across_the_octave_shift_stays_in_range() {
    let state = DivideDownState::new(48000.0);
    for registration in [0.0, 2.5, 3.7, 5.0] {
        for (carrier, modulator, depth) in [(2000.0, 1000.0, 3.0), (3000.0, 11000.0, 2.0)] {
            let mut voice = Voice::default();
            let mut peak = 0.0f32;
            for n in 0..48000 {
                let t = n as f32 / 48000.0;
                let fm = depth * (std::f32::consts::TAU * modulator * t).sin();
                let f0 = carrier * (1.0 + fm) / 48000.0;
                peak = peak.max(voice.step(f0, (registration, mark()), &state).abs());
            }
            assert!(
                peak <= 1.05,
                "registration {registration}, {carrier} Hz ± {depth} at {modulator} Hz: peak {peak}"
            );
        }
    }
}

/// Run forward then backward at the same speed, the oscillator retraces the
/// forward samples, edges and their corrections included.
#[test]
fn backward_retraces_forward() {
    let state = DivideDownState::new(48000.0);
    for (f0, registration) in [(0.0137, 0.0), (0.0137, 5.0), (0.031, 2.5), (0.0021, 3.7)] {
        let mut voice = Voice::default();
        let n = 4000;
        let forward: Vec<f32> = (0..n)
            .map(|_| voice.step(f0, (registration, mark()), &state))
            .collect();
        let backward: Vec<f32> = (0..n)
            .map(|_| voice.step(-f0, (registration, mark()), &state))
            .collect();
        // The output runs a sample late, so backward sample `k` retraces
        // forward sample `n − k`.
        let error = (1..n - 100)
            .map(|k| (backward[k] - forward[n - k]).abs())
            .fold(0.0f32, f32::max);
        assert!(
            error < 1e-3,
            "f0 {f0}, registration {registration}: {error}"
        );
    }
}

/// Notes past the octave shift (from about 3 kHz) have the same first four
/// harmonics at 44.1 and 96 kHz as at 48 kHz, within the few percent their
/// edges' residual aliasing differs between the rates; higher ones near
/// 44.1 kHz's Nyquist cannot match.
#[test]
fn high_notes_sound_the_same_at_any_rate() {
    for (pitch, registration) in [(3.6, 2.5), (3.9, 5.0), (3.55, 0.0)] {
        let hz = voct_to_hz(pitch);
        let reference = harmonics(&tone(48000.0, hz, registration, 0.5), hz, 48000.0, 4);
        let loudest = reference.iter().fold(0.0f32, |m, &a| m.max(a));
        for sample_rate in [44100.0, 96000.0] {
            let actual = harmonics(
                &tone(sample_rate, hz, registration, 0.5),
                hz,
                sample_rate,
                4,
            );
            for (h, (a, r)) in actual.iter().zip(&reference).enumerate() {
                assert!(
                    (a - r).abs() < 0.06 * loudest,
                    "{hz} Hz, registration {registration}, {sample_rate} Hz, harmonic {}: {a} vs {r}",
                    h + 1
                );
            }
        }
    }
}
