use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

/// Both outputs before the output gain, with room for `soft_clip` to round a
/// hair past 1.
const FULL_SCALE: f32 = 1.0 + 1e-6;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(cutoff: f32, resonance: f32, slope: f32, shape: f32) -> Knobs {
    Knobs {
        cutoff: (cutoff, mark()),
        resonance: (resonance, mark()),
        slope: (slope, mark()),
        shape: (shape, mark()),
    }
}

fn render(voice: &mut Voice, f0: f32, k: Knobs, state: &VcfState) -> (f32, f32) {
    let controls = voice.control(f0, f0, k, state);
    voice.step(controls, state)
}

/// Replays `vcf.f32` (Plaits' VCF engine on its own oscillators and filters,
/// with per-sample controls), low-pass and high-pass.
#[test]
fn matches_plaits() {
    let state = VcfState::new(48000.0);
    let mut voice = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("vcf", 5 + 2) {
        let (lp, hp) = render(
            &mut voice,
            record[0],
            knobs(record[1], record[2], record[3], record[4]),
            &state,
        );
        actual.extend([lp, hp]);
        expected.extend([record[5], record[6]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error == 0.0, "max error {error} at sample {}", at / 2);
}

/// Checked on both slopes, resonant and not, with the sub, and on the
/// high-pass output with the cutoff below the pitch; the pulses stay wide
/// enough to span several samples, which band-limit alike at any rate.
#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, cutoff, resonance, slope, shape, highpass) in [
        (-1.0, 4.0, 0.0, 0.0, 1.25, false),
        (0.0, 4.0, 1.0, 5.0, 2.5, false),
        (-2.0, 4.5, 0.0, 2.0, 0.0, false),
        (-1.0, 2.5, 4.0, 0.0, 1.25, false),
        (-1.0, 2.5, 4.0, 5.0, 2.5, false),
        (-1.0, 0.5, 0.0, 0.0, 1.25, true),
        (0.0, 0.5, 2.0, 5.0, 3.25, true),
    ] {
        assert_rate_independent(
            |sample_rate| {
                let state = VcfState::new(sample_rate);
                let mut voice = Voice::default();
                let f0 = voct_to_hz(pitch) / sample_rate;
                (0..(sample_rate * 0.5) as usize)
                    .map(|_| {
                        let (lp, hp) = render(
                            &mut voice,
                            f0,
                            knobs(cutoff, resonance, slope, shape),
                            &state,
                        );
                        if highpass { hp } else { lp }
                    })
                    .collect()
            },
            voct_to_hz(pitch),
            0.02,
        );
    }
}

/// A pitch running backward plays the waveform reversed, which the filter
/// shapes alike: the harmonics match the forward ones.
#[test]
fn runs_backward() {
    let sample_rate = 48000.0;
    let state = VcfState::new(sample_rate);
    for (hz, shape) in [(110.0, 1.25), (220.0, 2.5), (55.0, 3.75), (110.0, 0.0)] {
        let tone = |f0: f32| {
            let mut voice = Voice::default();
            let out: Vec<f32> = (0..48000)
                .map(|_| render(&mut voice, f0, knobs(3.5, 0.0, 0.0, shape), &state).0)
                .collect();
            harmonics(&out[4800..], hz, sample_rate, 16)
        };
        let forward = tone(hz / sample_rate);
        let backward = tone(-hz / sample_rate);
        let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
        for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
            assert!(
                (b - f).abs() < 0.03 * loudest,
                "{hz} Hz, shape {shape}, harmonic {}: {b} vs {f}",
                h + 1
            );
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = VcfState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..200 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 1.0,
        };
        let a = render(&mut held, 0.01, knobs(1.0, 1.0, 1.0, 1.0), &state);
        let b = render(&mut glitched, 0.01, knobs(v, v, v, v), &state);
        assert!(
            (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6,
            "sample {n}: {a:?} vs {b:?}"
        );
    }
}

/// Through-zero sweeps, standstills, knob jumps, non-finite pitches and the
/// frequency limit at any engine rate leave both outputs finite and within
/// full scale, and a steady note afterward still swings across zero below the
/// rail.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 24000.0, 44100.0, 48000.0, 96000.0] {
        let state = VcfState::new(sample_rate);
        let mut voice = Voice::default();
        for n in 0..6000 {
            let f0 = match n % 7 {
                0 => 2.0,
                1 => -2.0,
                2 => 0.0,
                3 if n % 500 == 3 => f32::NAN,
                4 if n % 500 == 4 => f32::INFINITY,
                _ => (n as f32 * 0.003).sin() * 0.02,
            };
            let knob = (n % 11) as f32 * 0.5;
            let (lp, hp) = render(&mut voice, f0, knobs(knob, 5.0 - knob, knob, knob), &state);
            assert!(
                lp.abs() <= FULL_SCALE && hp.abs() <= FULL_SCALE,
                "{sample_rate} Hz, sample {n}: {lp} {hp}"
            );
        }
        let (lp, hp): (Vec<f32>, Vec<f32>) = (0..(sample_rate * 0.25) as usize)
            .map(|_| {
                render(
                    &mut voice,
                    220.0 / sample_rate,
                    knobs(3.0, 0.0, 0.0, 1.25),
                    &state,
                )
            })
            .unzip();
        assert_swings(&lp, &format!("{sample_rate} Hz low-pass"));
        assert_swings(&hp, &format!("{sample_rate} Hz high-pass"));
    }
}

/// `x` crosses zero over and over and its RMS stays below full scale:
/// a filter stuck at a rail or on a DC offset fails.
fn assert_swings(x: &[f32], name: &str) {
    let x = &x[x.len() / 4..];
    let crossings = x
        .windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count();
    let rms = (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
    assert!(crossings > 20, "{name}: {crossings} zero crossings");
    assert!(rms < 0.95, "{name}: rms {rms}");
}

/// At full resonance on either slope, with the cutoff sweeping the whole
/// range, the filter rings but stays within full scale and keeps swinging.
#[test]
fn full_resonance_stays_bounded() {
    let state = VcfState::new(48000.0);
    for slope in [0.0, 5.0] {
        let mut voice = Voice::default();
        let (lp, hp): (Vec<f32>, Vec<f32>) = (0..48000)
            .map(|n| {
                let cutoff = 2.5 + 2.5 * (n as f32 * 0.0005).sin();
                render(&mut voice, 0.004, knobs(cutoff, 5.0, slope, 2.5), &state)
            })
            .unzip();
        for (n, (l, h)) in lp.iter().zip(&hp).enumerate() {
            assert!(l.abs() <= FULL_SCALE && h.abs() <= FULL_SCALE, "sample {n}");
        }
        assert_swings(&lp, &format!("slope {slope} low-pass"));
        assert_swings(&hp, &format!("slope {slope} high-pass"));
    }
}

/// The cutoff follows the note, not the oscillator: `fm`, even through zero,
/// leaves it where the note puts it.
#[test]
fn cutoff_ignores_fm() {
    let state = VcfState::new(48000.0);
    let note = 220.0 / 48000.0;
    let mut steady = Voice::default();
    let mut modulated = Voice::default();
    for n in 0..1000 {
        let f0 = note * (1.0 + 3.0 * (n as f32 * 0.05).sin());
        let a = steady.control(note, note, knobs(2.5, 3.0, 0.0, 1.25), &state);
        let b = modulated.control(f0, note, knobs(2.5, 3.0, 0.0, 1.25), &state);
        assert_eq!(a.cutoff, b.cutoff, "sample {n}");
    }
}
