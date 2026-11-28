use super::*;
use crate::dsp::mi::core::control::Ramp;
use crate::dsp::mi::core::fixture::{assert_rate_independent, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

/// Replays `grain.f32` (the Z oscillator of Plaits' granular formant engine)
/// through a voice ramping its pitch block by block as Plaits does.
#[test]
fn matches_plaits() {
    let state = ZOscState::new(48000.0);
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
            cutoff: (record[4], mark()),
            shape: (record[5], mark()),
            mode: (record[6], mark()),
            offset: (0.0, mark()),
        };
        voice.control(record[0], knobs, 12.0, 48000.0, &state);
        // Plaits ramps to a limited pitch, so the replayed ramp limits its targets.
        f0.set(record[0].min(0.125), Some(12.0));
        for &plaits in &record[19..31] {
            actual.push(voice.step(f0.next_value(), &state) / OUTPUT_GAIN);
            expected.push(plaits);
        }
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {at}");
}

/// Under through-zero FM the waveform runs backward: the output stays bounded
/// and its band-limited restarts never jump.
#[test]
fn runs_backward() {
    let state = ZOscState::new(48000.0);
    for f0 in [-0.003, -0.02, -0.11] {
        let knobs = || Knobs {
            cutoff: (2.0, mark()),
            shape: (1.0, mark()),
            mode: (1.0, mark()),
            offset: (0.0, mark()),
        };
        let mut voice = Voice::default();
        voice.control(f0, knobs(), 1.0, 48000.0, &state);
        let out: Vec<f32> = (0..4800).map(|_| voice.step(f0, &state)).collect();
        let mut forward = Voice::default();
        forward.control(-f0, knobs(), 1.0, 48000.0, &state);
        let reference: Vec<f32> = (0..4800).map(|_| forward.step(-f0, &state)).collect();
        let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let jump = |x: &[f32]| x.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        assert!(out.iter().all(|v| v.is_finite()));
        assert!(peak(&out) < 1.5 * peak(&reference), "f0 {f0}");
        assert!(
            jump(&out) < 1.5 * jump(&reference),
            "f0 {f0} jump {} vs {} peak {} vs {}",
            jump(&out),
            jump(&reference),
            peak(&out),
            peak(&reference)
        );
    }
}

#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, cutoff, shape, mode) in [(-1.0, 2.0, 1.0, 1.0), (2.0, 4.0, 4.0, 4.0)] {
        assert_rate_independent(
            |sample_rate| {
                let state = ZOscState::new(sample_rate);
                let mut voice = Voice::default();
                let f0 = voct_to_hz(pitch) / sample_rate;
                (0..(sample_rate * 0.5) as usize)
                    .map(|_| {
                        let knobs = Knobs {
                            cutoff: (cutoff, mark()),
                            shape: (shape, mark()),
                            mode: (mode, mark()),
                            offset: (0.0, mark()),
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

/// A carrier stalled just below zero holds its waveform still.
#[test]
fn holds_still_just_below_zero() {
    let state = ZOscState::new(48000.0);
    let mut voice = Voice::default();
    let knobs = Knobs {
        cutoff: (2.0, mark()),
        shape: (1.0, mark()),
        mode: (1.0, mark()),
        offset: (0.0, mark()),
    };
    voice.control(-1e-9, knobs, 1.0, 48000.0, &state);
    let out: Vec<f32> = (0..64).map(|_| voice.step(-1e-9, &state)).collect();
    let ripple = out[8..]
        .windows(2)
        .fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    assert!(ripple < 1e-3, "ripple {ripple}");
}

/// A knob change is heard on the very next sample.
#[test]
fn knobs_take_effect_on_the_next_sample() {
    let state = ZOscState::new(48000.0);
    let f0 = 0.01;
    let knobs = |cutoff: f32| Knobs {
        cutoff: (cutoff, mark()),
        shape: (1.0, mark()),
        mode: (2.0, mark()),
        offset: (0.0, mark()),
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
    let state = ZOscState::new(48000.0);
    let f0 = 0.01;
    let knobs = |v: f32| Knobs {
        cutoff: (v, mark()),
        shape: (v, mark()),
        mode: (v, mark()),
        offset: (0.0, mark()),
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

fn offset_knobs(cutoff: f32, shape: f32, mode: f32, offset: f32) -> Knobs {
    Knobs {
        cutoff: (cutoff, mark()),
        shape: (shape, mark()),
        mode: (mode, mark()),
        offset: (offset, mark()),
    }
}

fn render(voice: &mut Voice, f0: f32, knobs: Knobs, state: &ZOscState) -> f32 {
    voice.control(f0, knobs, 1.0, 48000.0, state);
    voice.step(f0, state)
}

/// A phase offset plays the same waveform shifted in time, forward or
/// backward: at a 100-sample period, a quarter-cycle offset leads by 25
/// samples going forward, and three quarters lead by 25 going backward.
#[test]
fn phase_offset_shifts_the_waveform_in_time() {
    let state = ZOscState::new(48000.0);
    for f0 in [0.01, -0.01] {
        for (cutoff, shape, mode) in [(2.5, 2.5, 2.5), (4.0, 0.5, 1.0), (1.0, 4.5, 4.5)] {
            let tone = |offset: f32| {
                let mut voice = Voice::default();
                (0..4000)
                    .map(|_| {
                        render(
                            &mut voice,
                            f0,
                            offset_knobs(cutoff, shape, mode, offset),
                            &state,
                        )
                    })
                    .collect::<Vec<f32>>()
            };
            let shifted = tone(if f0 > 0.0 { 0.25 } else { 0.75 });
            let plain = tone(0.0);
            let (error, at) = max_error(&shifted[1000..3000], &plain[1025..3025]);
            assert!(
                error < 1e-3,
                "f0 {f0}, cutoff {cutoff}: error {error} at {at}"
            );
        }
    }
}

/// An offset that moves, or jumps, ends where a voice started at it plays,
/// within the rounding its phases gather on the way.
#[test]
fn moving_phase_offset_lands_on_the_held_one() {
    let state = ZOscState::new(48000.0);
    let mut held = Voice::default();
    let mut glided = Voice::default();
    let mut jumped = Voice::default();
    for n in 0..6000 {
        let glide = (n as f32 / 2000.0).min(1.0) * 0.4;
        let jump = if n < 100 { 0.9 } else { 0.4 };
        let a = render(&mut held, 0.01, offset_knobs(3.0, 1.5, 2.0, 0.4), &state);
        let b = render(
            &mut glided,
            0.01,
            offset_knobs(3.0, 1.5, 2.0, glide),
            &state,
        );
        let c = render(&mut jumped, 0.01, offset_knobs(3.0, 1.5, 2.0, jump), &state);
        if n >= 3000 {
            assert!((a - b).abs() < 1e-2, "glided, sample {n}: {a} vs {b}");
            assert!((a - c).abs() < 1e-2, "jumped, sample {n}: {a} vs {c}");
        }
    }
}

/// A NaN or infinite phase offset holds the oscillator where it is.
#[test]
fn non_finite_phase_offset_holds() {
    let state = ZOscState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..200 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            _ => 0.3,
        };
        let a = render(&mut held, 0.01, offset_knobs(2.0, 1.0, 2.0, 0.3), &state);
        let b = render(&mut glitched, 0.01, offset_knobs(2.0, 1.0, 2.0, v), &state);
        assert!((a - b).abs() < 1e-6, "sample {n}: {a} vs {b}");
    }
}

/// Wild pitches, knobs and phase offsets never make the output non-finite.
#[test]
fn stays_finite() {
    for sample_rate in [48000.0, 192000.0] {
        let state = ZOscState::new(sample_rate);
        let mut voice = Voice::default();
        for n in 0..20000 {
            let f0 = match n % 7 {
                0 => 0.0,
                3 => -0.4,
                _ => (n as f32 * 0.003).sin() * 0.2,
            };
            let knob = (n % 11) as f32 * 0.5;
            let offset = match n % 13 {
                0 if n % 600 == 0 => f32::NAN,
                1 if n % 600 == 1 => f32::NEG_INFINITY,
                5 => -7.25,
                _ => (n as f32 * 0.007).sin() * 3.0,
            };
            voice.control(
                f0,
                offset_knobs(knob, 5.0 - knob, knob, offset),
                1.0,
                sample_rate,
                &state,
            );
            let out = voice.step(f0, &state);
            assert!(out.is_finite(), "{sample_rate} Hz, sample {n}");
        }
    }
}
