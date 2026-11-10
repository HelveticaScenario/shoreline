use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(ratio: f32, index: f32, feedback: f32) -> Knobs {
    Knobs {
        ratio: (ratio, mark()),
        index: (index, mark()),
        feedback: (feedback, mark()),
    }
}

/// Replays `two_op.f32` (Plaits' two-operator FM engine with per-sample
/// controls), including a carrier sweeping through zero.
#[test]
fn matches_plaits() {
    let state = TwoOpState::new(48000.0);
    let mut voice = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("two_op", 4 + 2) {
        let controls = voice.control(record[0], knobs(record[1], record[2], record[3]), &state);
        let (out, sub) = voice.step(controls, &state);
        actual.extend([out, sub]);
        expected.extend([record[4], record[5]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// A carrier running backward plays the forward waveform mirrored.
#[test]
fn runs_backward() {
    let state = TwoOpState::new(48000.0);
    for (f0, feedback) in [(0.003, 0.0), (0.02, 3.0), (0.11, 0.0)] {
        let render = |f0: f32| {
            let mut voice = Voice::default();
            (0..4800)
                .map(|_| {
                    let controls = voice.control(f0, knobs(3.0, 3.0, feedback), &state);
                    voice.step(controls, &state)
                })
                .collect::<Vec<_>>()
        };
        let forward = render(f0);
        let backward = render(-f0);
        let error = forward.iter().zip(&backward).fold(0.0f32, |m, (a, b)| {
            m.max((a.0 + b.0).abs()).max((a.1 + b.1).abs())
        });
        assert!(error < 1e-3, "f0 {f0}: mirror error {error}");
    }
}

#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, ratio, index, feedback) in [
        (-1.0, 2.5, 2.0, 0.0),
        (1.0, 4.0, 3.0, 2.0),
        (0.0, 2.0, 2.5, -3.0),
    ] {
        for sub in [false, true] {
            assert_rate_independent(
                |sample_rate| {
                    let state = TwoOpState::new(sample_rate);
                    let mut voice = Voice::default();
                    let f0 = voct_to_hz(pitch) / sample_rate;
                    (0..(sample_rate * 0.5) as usize)
                        .map(|_| {
                            let controls = voice.control(f0, knobs(ratio, index, feedback), &state);
                            let (out, sub_octave) = voice.step(controls, &state);
                            if sub { sub_octave } else { out }
                        })
                        .collect()
                },
                voct_to_hz(pitch) * if sub { 0.5 } else { 1.0 },
                0.02,
            );
        }
    }
}

/// A knob change is heard on the very next sample.
#[test]
fn knobs_take_effect_on_the_next_sample() {
    let state = TwoOpState::new(48000.0);
    let mut held = Voice::default();
    let mut moved = Voice::default();
    for n in 0..52 {
        let index = if n < 50 { 1.0 } else { 3.0 };
        let a = held.control(0.01, knobs(2.5, 1.0, 0.0), &state);
        let b = moved.control(0.01, knobs(2.5, index, 0.0), &state);
        let (a, b) = (held.step(a, &state).0, moved.step(b, &state).0);
        if n < 50 {
            assert_eq!(a, b, "sample {n}");
        } else {
            assert_ne!(a, b, "sample {n}");
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = TwoOpState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..200 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 1.0,
        };
        let a = held.control(0.01, knobs(1.0, 1.0, 1.0), &state);
        let b = glitched.control(0.01, knobs(v, v, v), &state);
        let (a, b) = (held.step(a, &state).0, glitched.step(b, &state).0);
        assert!((a - b).abs() < 1e-4, "sample {n}: {a} vs {b}");
    }
}

/// A NaN or infinite pitch leaves the output finite.
#[test]
fn non_finite_pitch_stays_finite() {
    let state = TwoOpState::new(48000.0);
    let mut voice = Voice::default();
    for n in 0..300 {
        let f0 = match n {
            100 => f32::NAN,
            150 => f32::INFINITY,
            200 => f32::NEG_INFINITY,
            _ => 0.01,
        };
        let controls = voice.control(f0, knobs(2.5, 3.0, -3.0), &state);
        let (out, sub) = voice.step(controls, &state);
        assert!(out.is_finite() && sub.is_finite(), "sample {n}");
    }
}
