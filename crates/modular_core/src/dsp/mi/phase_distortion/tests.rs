use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(ratio: f32, amount: f32, asymmetry: f32) -> Knobs {
    Knobs {
        ratio: (ratio, mark()),
        amount: (amount, mark()),
        asymmetry: (asymmetry, mark()),
    }
}

fn render(voice: &mut Voice, f0: f32, k: Knobs, sync: bool, state: &PhaseDistortionState) -> f32 {
    let controls = voice.control(f0, k, state);
    voice.step(controls, sync, state)
}

/// Replays `phase_distortion.f32` (Plaits' phase distortion engine on its own
/// oscillator, with per-sample controls), synced and free-running.
#[test]
fn matches_plaits() {
    let state = PhaseDistortionState::new(48000.0);
    let mut synced = Voice::default();
    let mut free = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("phase_distortion", 4 + 2) {
        let k = || knobs(record[1], record[2], record[3]);
        actual.push(render(&mut synced, record[0], k(), true, &state));
        actual.push(render(&mut free, record[0], k(), false, &state));
        expected.extend([record[4], record[5]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// A pitch running backward plays the forward waveform reversed in time, so
/// its harmonics match the forward ones. The triangle reaches the output one
/// substep after the phasor, as in Plaits, which reversal turns into a lead,
/// so the match is close at a gentle `amount` and loosens as it rises.
#[test]
fn runs_backward() {
    let sample_rate = 48000.0;
    let state = PhaseDistortionState::new(sample_rate);
    for sync in [true, false] {
        for (hz, ratio, asymmetry) in [
            (110.0, 2.5, 0.0),
            (220.0, 4.0, 3.0),
            (55.0, 1.0, 5.0),
            (440.0, 3.75, 1.5),
        ] {
            let tone = |f0: f32| {
                let mut voice = Voice::default();
                let out: Vec<f32> = (0..48000)
                    .map(|_| render(&mut voice, f0, knobs(ratio, 1.0, asymmetry), sync, &state))
                    .collect();
                assert!(out.iter().all(|v| v.is_finite()));
                harmonics(&out[4800..], hz, sample_rate, 16)
            };
            let forward = tone(hz / sample_rate);
            let backward = tone(-hz / sample_rate);
            let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
            for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
                assert!(
                    (b - f).abs() < 0.05 * loudest,
                    "sync {sync}, {hz} Hz, harmonic {}: {b} vs {f}",
                    h + 1
                );
            }
        }
    }
}

/// Checked on harmonic tones: synced, or free-running at a ×2 or ×4 ratio
/// (the table's plateaus at 2.383 and 3.75), and gentle enough not to alias
/// at 44.1 kHz.
#[test]
fn sounds_the_same_at_any_rate() {
    for (sync, pitch, ratio, amount, asymmetry) in [
        (true, -1.0, 2.5, 2.0, 0.0),
        (true, 0.0, 3.5, 2.0, 3.0),
        (false, -1.0, 2.383, 2.0, 0.0),
        (false, 0.0, 3.75, 2.0, 3.0),
    ] {
        assert_rate_independent(
            |sample_rate| {
                let state = PhaseDistortionState::new(sample_rate);
                let mut voice = Voice::default();
                let f0 = voct_to_hz(pitch) / sample_rate;
                (0..(sample_rate * 0.5) as usize)
                    .map(|_| {
                        render(
                            &mut voice,
                            f0,
                            knobs(ratio, amount, asymmetry),
                            sync,
                            &state,
                        )
                    })
                    .collect()
            },
            voct_to_hz(pitch),
            0.02,
        );
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = PhaseDistortionState::new(48000.0);
    for sync in [true, false] {
        let mut held = Voice::default();
        let mut glitched = Voice::default();
        for n in 0..200 {
            let v = match n {
                100 => f32::NAN,
                101 => f32::INFINITY,
                102 => f32::NEG_INFINITY,
                _ => 1.0,
            };
            let a = render(&mut held, 0.01, knobs(1.0, 1.0, 1.0), sync, &state);
            let b = render(&mut glitched, 0.01, knobs(v, v, v), sync, &state);
            assert!((a - b).abs() < 1e-6, "sync {sync}, sample {n}: {a} vs {b}");
        }
    }
}

/// Through-zero sweeps, standstills, knob jumps, non-finite pitches and the
/// frequency limit at any engine rate leave the output finite and bounded.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 24000.0, 44100.0, 48000.0, 96000.0] {
        let state = PhaseDistortionState::new(sample_rate);
        for sync in [true, false] {
            let mut voice = Voice::default();
            for n in 0..6000 {
                let f0 = match n % 7 {
                    0 => 2.0 * state.max_frequency,
                    1 => -2.0 * state.max_frequency,
                    2 => 0.0,
                    3 if n % 500 == 3 => f32::NAN,
                    4 if n % 500 == 4 => f32::INFINITY,
                    _ => (n as f32 * 0.003).sin() * 0.02,
                };
                let knob = (n % 11) as f32 * 0.5;
                let out = render(&mut voice, f0, knobs(knob, 5.0 - knob, knob), sync, &state);
                assert!(out.is_finite(), "{sample_rate} Hz, sync {sync}, sample {n}");
            }
        }
    }
}

/// Holding the frequency limit, which passes a whole cycle per substep below
/// 12 kHz, leaves no phase behind: the tone after it matches a fresh one.
#[test]
fn recovers_from_the_frequency_limit() {
    for sample_rate in [8000.0, 11025.0, 48000.0] {
        let state = PhaseDistortionState::new(sample_rate);
        for sync in [true, false] {
            for limit in [2.0 * state.max_frequency, -2.0 * state.max_frequency] {
                let hz = 220.0;
                let tone = |voice: &mut Voice| {
                    let out: Vec<f32> = (0..(sample_rate * 0.5) as usize)
                        .map(|_| {
                            render(voice, hz / sample_rate, knobs(2.5, 2.0, 1.0), sync, &state)
                        })
                        .collect();
                    harmonics(&out[(sample_rate * 0.1) as usize..], hz, sample_rate, 8)
                };
                let fresh = tone(&mut Voice::default());
                let mut voice = Voice::default();
                for _ in 0..sample_rate as usize {
                    render(&mut voice, limit, knobs(2.5, 2.0, 1.0), sync, &state);
                }
                let after = tone(&mut voice);
                let loudest = fresh.iter().fold(0.0f32, |m, &a| m.max(a));
                for (h, (a, f)) in after.iter().zip(&fresh).enumerate() {
                    assert!(
                        (a - f).abs() < 0.05 * loudest,
                        "{sample_rate} Hz, sync {sync}, limit {limit}, harmonic {}: {a} vs {f}",
                        h + 1
                    );
                }
            }
        }
    }
}

/// At a strong `amount` backward departs from the mirror only by the
/// triangle's one-substep lag, so the departure shrinks as the rate rises.
#[test]
fn backward_departs_only_by_the_substep_lag() {
    let departure = |sample_rate: f32| {
        let state = PhaseDistortionState::new(sample_rate);
        let hz = 220.0;
        let tone = |f0: f32| {
            let mut voice = Voice::default();
            let out: Vec<f32> = (0..sample_rate as usize)
                .map(|_| render(&mut voice, f0, knobs(4.0, 5.0, 3.0), true, &state))
                .collect();
            harmonics(&out[(sample_rate * 0.1) as usize..], hz, sample_rate, 16)
        };
        let forward = tone(hz / sample_rate);
        let backward = tone(-hz / sample_rate);
        let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
        forward
            .iter()
            .zip(&backward)
            .fold(0.0f32, |m, (f, b)| m.max((f - b).abs()))
            / loudest
    };
    let at_48k = departure(48000.0);
    let at_192k = departure(192000.0);
    assert!(
        at_192k < 0.4 * at_48k,
        "departure {at_192k} at 192 kHz vs {at_48k} at 48 kHz"
    );
}
