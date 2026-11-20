use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(detune: f32, square: f32, shape: f32) -> Knobs {
    Knobs {
        detune: (detune, mark()),
        square: (square, mark()),
        shape: (shape, mark()),
    }
}

/// Replays `va.f32` (Plaits' virtual analog engine on its own oscillators,
/// with per-sample controls).
#[test]
fn matches_plaits() {
    let state = VirtualAnalogState::new(48000.0);
    let mut voice = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("va", 4 + 2) {
        let (dual, sync) = voice.step(record[0], knobs(record[1], record[2], record[3]), &state);
        actual.extend([dual, sync]);
        expected.extend([record[4], record[5]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// Unison, so both outputs are tones at `hz`, each with its own settings.
fn tone(
    voice: &mut Voice,
    f0: f32,
    square: f32,
    shape: f32,
    state: &VirtualAnalogState,
) -> (f32, f32) {
    voice.step(f0, knobs(2.5, square, shape), state)
}

/// A pitch running backward plays the forward waveforms reversed in time, so
/// their harmonics match the forward ones.
#[test]
fn runs_backward() {
    let sample_rate = 48000.0;
    let state = VirtualAnalogState::new(sample_rate);
    for (hz, square, shape) in [
        (110.0, 2.5, 2.5),
        (220.0, 4.0, 1.0),
        (55.0, 1.0, 4.0),
        (440.0, 3.5, 3.0),
    ] {
        let render = |f0: f32| {
            let mut voice = Voice::default();
            let (dual, sync): (Vec<f32>, Vec<f32>) = (0..48000)
                .map(|_| tone(&mut voice, f0, square, shape, &state))
                .unzip();
            assert!(dual.iter().chain(&sync).all(|v| v.is_finite()));
            [
                harmonics(&dual[4800..], hz, sample_rate, 16),
                harmonics(&sync[4800..], hz, sample_rate, 16),
            ]
        };
        let forward = render(hz / sample_rate);
        let backward = render(-hz / sample_rate);
        for (output, (f, b)) in ["dual", "sync"].iter().zip(forward.iter().zip(&backward)) {
            let loudest = f.iter().fold(0.0f32, |m, &a| m.max(a));
            for (h, (b, f)) in b.iter().zip(f).enumerate() {
                assert!(
                    (b - f).abs() < 0.02 * loudest,
                    "{output}, {hz} Hz, harmonic {}: {b} vs {f}",
                    h + 1
                );
            }
        }
    }
}

/// Checked on harmonic tones at unison, gentle enough not to alias at
/// 44.1 kHz.
#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, square, shape) in [(-1.0, 2.5, 2.5), (0.0, 1.5, 1.0), (-1.0, 3.5, 3.5)] {
        for output in [0, 1] {
            assert_rate_independent(
                |sample_rate| {
                    let state = VirtualAnalogState::new(sample_rate);
                    let mut voice = Voice::default();
                    let f0 = voct_to_hz(pitch) / sample_rate;
                    (0..(sample_rate * 0.5) as usize)
                        .map(|_| {
                            let (dual, sync) = tone(&mut voice, f0, square, shape, &state);
                            if output == 0 { dual } else { sync }
                        })
                        .collect()
                },
                voct_to_hz(pitch),
                0.02,
            );
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = VirtualAnalogState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..200 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 3.0,
        };
        let a = held.step(0.01, knobs(3.0, 3.0, 3.0), &state);
        let b = glitched.step(0.01, knobs(v, v, v), &state);
        assert!(
            (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6,
            "sample {n}: {a:?} vs {b:?}"
        );
    }
}

/// Through-zero sweeps, standstills, knob jumps, non-finite pitches and the
/// frequency limit at any engine rate leave the outputs finite and bounded.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 24000.0, 44100.0, 48000.0, 96000.0] {
        let state = VirtualAnalogState::new(sample_rate);
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
            let (dual, sync) = voice.step(f0, knobs(knob, 5.0 - knob, knob), &state);
            for out in [dual, sync] {
                assert!(
                    out.is_finite() && out.abs() < 4.0,
                    "{sample_rate} Hz, sample {n}: {out}"
                );
            }
        }
    }
}

/// Holding the frequency limit, which passes a whole cycle per sample below
/// 12 kHz, leaves no phase behind: the tone after it matches a fresh one.
#[test]
fn recovers_from_the_frequency_limit() {
    for sample_rate in [8000.0, 11025.0, 48000.0] {
        let state = VirtualAnalogState::new(sample_rate);
        for limit in [2.0 * state.max_frequency, -2.0 * state.max_frequency] {
            let hz = 220.0;
            let render = |voice: &mut Voice| {
                let (dual, sync): (Vec<f32>, Vec<f32>) = (0..(sample_rate * 0.5) as usize)
                    .map(|_| tone(voice, hz / sample_rate, 2.5, 1.0, &state))
                    .unzip();
                let skip = (sample_rate * 0.1) as usize;
                [
                    harmonics(&dual[skip..], hz, sample_rate, 8),
                    harmonics(&sync[skip..], hz, sample_rate, 8),
                ]
            };
            let fresh = render(&mut Voice::default());
            let mut voice = Voice::default();
            for _ in 0..sample_rate as usize {
                tone(&mut voice, limit, 2.5, 1.0, &state);
            }
            let after = render(&mut voice);
            for (f, a) in fresh.iter().zip(&after) {
                let loudest = f.iter().fold(0.0f32, |m, &x| m.max(x));
                for (h, (a, f)) in a.iter().zip(f).enumerate() {
                    assert!(
                        (a - f).abs() < 0.05 * loudest,
                        "{sample_rate} Hz, limit {limit}, harmonic {}: {a} vs {f}",
                        h + 1
                    );
                }
            }
        }
    }
}

/// `detune` lingers on unison, fifths and octaves.
#[test]
fn detune_lingers_on_intervals() {
    for (volts, semitones) in [
        (2.5, 0.0),
        (0.0, -24.01),
        (5.0, 24.01),
        (3.11, 7.01),
        (1.89, -7.01),
    ] {
        let actual = detuning(knob(volts));
        assert!((actual - semitones).abs() < 0.05, "{volts} V: {actual}");
    }
}
