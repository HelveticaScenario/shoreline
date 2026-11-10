use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(shape: f32, fold: f32, asymmetry: f32) -> Knobs {
    Knobs {
        shape: (shape, mark()),
        fold: (fold, mark()),
        asymmetry: (asymmetry, mark()),
    }
}

fn render(voice: &mut Voice, f0: f32, k: Knobs, state: &WaveshaperState) -> (f32, f32) {
    let controls = voice.control(f0, k, state);
    voice.step(controls)
}

/// Replays `waveshaper.f32` (Plaits' waveshaping engine on its own
/// oscillator, with per-sample controls), including a pitch sweeping through
/// zero.
#[test]
fn matches_plaits() {
    let state = WaveshaperState::new(48000.0);
    let mut voice = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("waveshaper", 4 + 2) {
        let (out, overtones) = render(
            &mut voice,
            record[0],
            knobs(record[1], record[2], record[3]),
            &state,
        );
        actual.extend([out, overtones]);
        expected.extend([record[4], record[5]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// A pitch running backward plays the forward waveform mirrored in time,
/// which a symmetric wave makes the same tone.
#[test]
fn runs_backward() {
    let state = WaveshaperState::new(48000.0);
    for f0 in [0.003, 0.02, 0.11] {
        let peak_jump = |f0: f32| {
            let mut voice = Voice::default();
            let out: Vec<f32> = (0..4800)
                .map(|_| render(&mut voice, f0, knobs(3.5, 3.0, 0.0), &state).0)
                .collect();
            assert!(out.iter().all(|v| v.is_finite()), "f0 {f0}");
            let peak = out[100..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let jump = out[100..]
                .windows(2)
                .fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
            (peak, jump)
        };
        let (peak, jump) = peak_jump(f0);
        let (back_peak, back_jump) = peak_jump(-f0);
        assert!(
            (back_peak - peak).abs() < 0.02,
            "f0 {f0}: peak {back_peak} vs {peak}"
        );
        assert!(
            back_jump < 1.2 * jump,
            "f0 {f0}: jump {back_jump} vs {jump}"
        );
    }
}

#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, shape, fold, asymmetry) in [(-1.0, 2.5, 2.0, 0.0), (1.0, 4.0, 3.0, 3.0)] {
        for overtones in [false, true] {
            assert_rate_independent(
                |sample_rate| {
                    let state = WaveshaperState::new(sample_rate);
                    let mut voice = Voice::default();
                    let f0 = voct_to_hz(pitch) / sample_rate;
                    (0..(sample_rate * 0.5) as usize)
                        .map(|_| {
                            let (out, o) =
                                render(&mut voice, f0, knobs(shape, fold, asymmetry), &state);
                            if overtones { o } else { out }
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
    let state = WaveshaperState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..200 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 1.0,
        };
        let a = render(&mut held, 0.01, knobs(1.0, 1.0, 1.0), &state).0;
        let b = render(&mut glitched, 0.01, knobs(v, v, v), &state).0;
        assert!((a - b).abs() < 1e-6, "sample {n}: {a} vs {b}");
    }
}

/// The peak jumping across the phase at a standstill, or a NaN or infinite
/// pitch, leaves the output finite.
#[test]
fn stays_finite() {
    let state = WaveshaperState::new(48000.0);
    let mut voice = Voice::default();
    for n in 0..400 {
        let f0 = match n {
            100..200 => 0.0,
            250 => f32::NAN,
            300 => f32::INFINITY,
            350 => f32::NEG_INFINITY,
            _ => 0.01,
        };
        let asymmetry = if n % 2 == 0 { 0.0 } else { 5.0 };
        let (out, overtones) = render(&mut voice, f0, knobs(3.0, 3.0, asymmetry), &state);
        assert!(out.is_finite() && overtones.is_finite(), "sample {n}");
        assert!(
            out.abs() <= 1.5 && overtones.abs() <= 1.5,
            "sample {n}: {out} {overtones}"
        );
    }
}

/// At engine rates where the frequency limit passes a quarter cycle per
/// sample, the top pitches and wild FM stay finite and bounded.
#[test]
fn stays_finite_at_low_rates() {
    for sample_rate in [8000.0, 16000.0, 22050.0, 24000.0, 44100.0] {
        let state = WaveshaperState::new(sample_rate);
        let mut voice = Voice::default();
        for n in 0..4000 {
            let f0 = match n % 4 {
                0 => state.max_frequency,
                1 => -state.max_frequency,
                2 => 10.0,
                _ => (n as f32 * 0.37).sin() * state.max_frequency,
            };
            let knob = (n % 11) as f32 * 0.5;
            let (out, overtones) = render(&mut voice, f0, knobs(knob, 5.0 - knob, knob), &state);
            assert!(
                out.is_finite() && overtones.is_finite(),
                "{sample_rate} Hz, sample {n}"
            );
            assert!(
                out.abs() <= 1.5 && overtones.abs() <= 1.5,
                "{sample_rate} Hz, sample {n}: {out} {overtones}"
            );
        }
    }
}
