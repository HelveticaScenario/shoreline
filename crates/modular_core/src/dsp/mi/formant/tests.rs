use super::*;
use crate::dsp::mi::core::control::Ramp;
use crate::dsp::mi::core::fixture::{assert_rate_independent, max_error, records};
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

/// Replays `grain.f32` (Plaits' granular formant engine) through a voice
/// ramping its pitch block by block as Plaits does.
#[test]
fn matches_plaits() {
    let state = FormantState::new(48000.0);
    // Plaits starts its interpolated controls from zero.
    let mut voice = Voice {
        started: true,
        ..Voice::default()
    };
    let mut f0 = Ramp::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("grain", 7 + 24) {
        let knobs = Knobs {
            formant: (record[1], mark()),
            ratio: (record[2], mark()),
            width: (record[3], mark()),
        };
        voice.control(record[0], knobs, 12.0, 48000.0, &state);
        // Plaits ramps to a limited pitch, so the replayed ramp limits its targets.
        f0.set(record[0].min(0.125), Some(12.0));
        for &plaits in &record[7..19] {
            actual.push(voice.step(f0.next_value(), &state) / OUTPUT_GAIN);
            expected.push(plaits);
        }
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {at}");
}

/// Under through-zero FM the carrier runs backward: the output stays bounded
/// and its band-limited restarts never jump.
#[test]
fn runs_backward() {
    let state = FormantState::new(48000.0);
    for f0 in [-0.003, -0.02, -0.11] {
        let mut voice = Voice::default();
        let knobs = || Knobs {
            formant: (1.0, mark()),
            ratio: (1.0, mark()),
            width: (4.0, mark()),
        };
        voice.control(f0, knobs(), 1.0, 48000.0, &state);
        let out: Vec<f32> = (0..4800).map(|_| voice.step(f0, &state)).collect();
        let mut forward = Voice::default();
        forward.control(-f0, knobs(), 1.0, 48000.0, &state);
        let reference: Vec<f32> = (0..4800).map(|_| forward.step(-f0, &state)).collect();
        let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let jump = |x: &[f32]| x.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        assert!(out.iter().all(|v| v.is_finite()));
        assert!(peak(&out) < 1.5 * peak(&reference), "f0 {f0}");
        assert!(jump(&out) < 1.5 * jump(&reference), "f0 {f0}");
    }
}

#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, formant, ratio, width) in [(-1.0, 1.0, 1.0, 1.0), (3.0, 2.0, 4.0, 4.0)] {
        assert_rate_independent(
            |sample_rate| {
                let state = FormantState::new(sample_rate);
                let mut voice = Voice::default();
                let f0 = voct_to_hz(pitch) / sample_rate;
                (0..(sample_rate * 0.5) as usize)
                    .map(|_| {
                        let knobs = Knobs {
                            formant: (formant, mark()),
                            ratio: (ratio, mark()),
                            width: (width, mark()),
                        };
                        voice.control(f0, knobs, 1.0, sample_rate, &state);
                        voice.step(f0, &state)
                    })
                    .collect()
            },
            voct_to_hz(pitch),
            0.02,
        );
    }
}

/// A knob change is heard on the very next sample.
#[test]
fn knobs_take_effect_on_the_next_sample() {
    let state = FormantState::new(48000.0);
    let f0 = 0.01;
    let knobs = |formant: f32| Knobs {
        formant: (formant, mark()),
        ratio: (1.0, mark()),
        width: (2.0, mark()),
    };
    let mut held = Voice::default();
    let mut moved = Voice::default();
    for n in 0..52 {
        held.control(f0, knobs(1.0), 1.0, 48000.0, &state);
        moved.control(
            f0,
            knobs(if n < 50 { 1.0 } else { 3.0 }),
            1.0,
            48000.0,
            &state,
        );
        let (a, b) = (held.step(f0, &state), moved.step(f0, &state));
        // The output runs one sample behind for its band-limited restarts.
        if n <= 50 {
            assert_eq!(a, b, "sample {n}");
        } else if n == 51 {
            assert_ne!(a, b);
        }
    }
}

/// A NaN knob holds the sound where it is.
#[test]
fn nan_knobs_hold() {
    let state = FormantState::new(48000.0);
    let f0 = 0.01;
    let knobs = |v: f32| Knobs {
        formant: (v, mark()),
        ratio: (v, mark()),
        width: (v, mark()),
    };
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..200 {
        let v = if (100..103).contains(&n) {
            f32::NAN
        } else {
            1.0
        };
        held.control(f0, knobs(1.0), 1.0, 48000.0, &state);
        glitched.control(f0, knobs(v), 1.0, 48000.0, &state);
        let (a, b) = (held.step(f0, &state), glitched.step(f0, &state));
        assert!((a - b).abs() < 1e-4, "sample {n}: {a} vs {b}");
    }
}
