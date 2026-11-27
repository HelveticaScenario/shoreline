use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(ratio: f32, index: f32, feedback: f32) -> Knobs {
    offset_knobs(ratio, index, feedback, 0.0)
}

fn offset_knobs(ratio: f32, index: f32, feedback: f32, offset: f32) -> Knobs {
    Knobs {
        ratio: (ratio, mark()),
        index: (index, mark()),
        feedback: (feedback, mark()),
        offset: (offset, mark()),
    }
}

/// The carrier and the sub-octave for one sample.
fn render(voice: &mut Voice, f0: f32, knobs: Knobs, state: &TwoOpState) -> (f32, f32) {
    let controls = voice.control(f0, knobs, state);
    voice.step(controls, state)
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

/// A NaN or infinite pitch or phase offset, or an offset that races around,
/// leaves the output finite.
#[test]
fn non_finite_pitch_stays_finite() {
    let state = TwoOpState::new(48000.0);
    let mut voice = Voice::default();
    for n in 0..3000 {
        let f0 = match n {
            100 => f32::NAN,
            150 => f32::INFINITY,
            200 => f32::NEG_INFINITY,
            _ => 0.01,
        };
        let offset = match n % 13 {
            0 if n % 600 == 0 => f32::NAN,
            1 if n % 600 == 1 => f32::NEG_INFINITY,
            5 => -7.25,
            _ => (n as f32 * 0.007).sin() * 3.0,
        };
        let feedback = if n < 1500 { -3.0 } else { 4.0 };
        let (out, sub) = render(
            &mut voice,
            f0,
            offset_knobs(2.5, 3.0, feedback, offset),
            &state,
        );
        assert!(out.is_finite() && sub.is_finite(), "sample {n}");
    }
}

/// A phase offset plays the same waveform shifted in time, carrier and
/// sub-octave alike: at a 100-sample period, a quarter-cycle offset leads by
/// 25 samples. Feedback into the modulator's phase keeps this; feedback into
/// its pitch carries a drift from the voice's own past, which no offset
/// recreates.
#[test]
fn phase_offset_shifts_the_waveform_in_time() {
    let state = TwoOpState::new(48000.0);
    for (ratio, index, feedback) in [(2.5, 3.0, 0.0), (3.7, 4.0, 0.0), (2.0, 2.5, 3.0)] {
        let tone = |offset: f32| {
            let mut voice = Voice::default();
            (0..4000)
                .map(|_| {
                    render(
                        &mut voice,
                        0.01,
                        offset_knobs(ratio, index, feedback, offset),
                        &state,
                    )
                })
                .collect::<Vec<_>>()
        };
        let ahead = tone(0.25);
        let plain = tone(0.0);
        for (output, pick) in [("fm", 0), ("subOctave", 1)] {
            let get = |s: &[(f32, f32)]| {
                s.iter()
                    .map(|&(a, b)| if pick == 0 { a } else { b })
                    .collect::<Vec<f32>>()
            };
            let (error, at) = max_error(&get(&ahead[1000..3000]), &get(&plain[1025..3025]));
            assert!(
                error < 1e-3,
                "{output}, ratio {ratio}, feedback {feedback}: error {error} at {at}"
            );
        }
    }
}

/// An offset that moves, or jumps, ends where a voice started at it plays,
/// within the rounding its phases gather on the way.
#[test]
fn moving_phase_offset_lands_on_the_held_one() {
    let state = TwoOpState::new(48000.0);
    for feedback in [0.0, 3.0] {
        let mut held = Voice::default();
        let mut glided = Voice::default();
        let mut jumped = Voice::default();
        for n in 0..6000 {
            let glide = (n as f32 / 2000.0).min(1.0) * 0.4;
            let jump = if n < 100 { 0.8 } else { 0.4 };
            let k = |offset| offset_knobs(3.7, 3.0, feedback, offset);
            let a = render(&mut held, 0.01, k(0.4), &state);
            let b = render(&mut glided, 0.01, k(glide), &state);
            let c = render(&mut jumped, 0.01, k(jump), &state);
            if n >= 3000 {
                for (name, other) in [("glided", b), ("jumped", c)] {
                    assert!(
                        (a.0 - other.0).abs() < 1e-3 && (a.1 - other.1).abs() < 1e-3,
                        "feedback {feedback}, {name}, sample {n}: {a:?} vs {other:?}"
                    );
                }
            }
        }
    }
}

/// How far phase `a` leads phase `b`, in cycles.
fn lead(a: u32, b: u32) -> f64 {
    f64::from(a.wrapping_sub(b).cast_signed()) / 4294967296.0
}

/// The carrier's, the modulator's and the sub-octave's phases.
fn phases(voice: &Voice) -> [u32; 3] {
    [voice.carrier_phase, voice.modulator_phase, voice.sub_phase]
}

/// An offset gliding across the wrap, at ratios that are not whole, moves
/// every phase smoothly and the output with no step beyond what a held offset
/// plays. Once across, the carrier is where a voice seeded at the new offset
/// is, the sub-octave half a cycle from it and the modulator the ratio's
/// fraction of a cycle; gliding back returns every phase to where it was.
#[test]
fn phase_offset_crosses_the_wrap_smoothly() {
    let state = TwoOpState::new(48000.0);
    // 0.5x, about 1.41x and 1.5x.
    for ratio in [0.0, 1.6, 1.9] {
        let k = |offset| offset_knobs(ratio, 3.0, 0.0, offset);
        let across = |n: usize| 0.95 + 0.1 * ((n as f32 - 500.0) / 1000.0).clamp(0.0, 1.0);
        let mut held = Voice::default();
        let mut glided = Voice::default();
        let mut seeded = Voice::default();
        let mut largest_step = [0.0f32; 2];
        let mut previous = [(0.0, 0.0); 2];
        let mut previous_lead = [0.0; 3];
        for n in 0..2000 {
            let a = render(&mut held, 0.01, k(0.95), &state);
            let b = render(&mut glided, 0.01, k(across(n)), &state);
            render(&mut seeded, 0.01, k(1.05), &state);
            for (i, (out, prev)) in [a, b].iter().zip(&mut previous).enumerate() {
                if n > 0 {
                    let step = (out.0 - prev.0).abs().max((out.1 - prev.1).abs());
                    largest_step[i] = largest_step[i].max(step);
                }
                *prev = *out;
            }
            for (i, (g, h)) in phases(&glided).into_iter().zip(phases(&held)).enumerate() {
                let now = lead(g, h);
                if n > 0 {
                    assert!(
                        (now - previous_lead[i]).abs() < 1e-3,
                        "ratio {ratio}, phase {i}, sample {n}: {} to {now}",
                        previous_lead[i]
                    );
                }
                previous_lead[i] = now;
            }
        }
        assert!(
            largest_step[1] < largest_step[0] * 1.05,
            "ratio {ratio}: steps {largest_step:?}"
        );
        let fraction = f64::from(glided.multiplier).fract();
        assert!(
            fraction > 0.1,
            "ratio {ratio}: multiplier {}",
            glided.multiplier
        );
        let [carrier, modulator, sub] = phases(&glided);
        let [seeded_carrier, seeded_modulator, seeded_sub] = phases(&seeded);
        assert!(lead(carrier, seeded_carrier).abs() < 1e-5);
        assert!((lead(sub, seeded_sub).abs() - 0.5).abs() < 1e-5);
        assert!(
            lead(
                modulator,
                seeded_modulator.wrapping_add(phase_increment(fraction as f32))
            )
            .abs()
                < 1e-5,
            "ratio {ratio}"
        );

        for n in 0..2000 {
            render(&mut glided, 0.01, k(across(1999 - n)), &state);
            render(&mut held, 0.01, k(0.95), &state);
        }
        for (g, h) in phases(&glided).into_iter().zip(phases(&held)) {
            assert!(lead(g, h).abs() < 1e-5, "ratio {ratio}");
        }
    }
}

/// A NaN or infinite phase offset holds the phases where they are.
#[test]
fn non_finite_phase_offset_holds() {
    let state = TwoOpState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..200 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 0.3,
        };
        let a = render(&mut held, 0.01, offset_knobs(2.5, 3.0, 1.0, 0.3), &state);
        let b = render(&mut glitched, 0.01, offset_knobs(2.5, 3.0, 1.0, v), &state);
        assert_eq!(a, b, "sample {n}");
    }
}
