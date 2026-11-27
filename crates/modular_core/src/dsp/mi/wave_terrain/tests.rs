use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(terrain: f32, radius: f32, offset: f32) -> Knobs {
    phase_knobs(terrain, radius, offset, 0.0)
}

fn phase_knobs(terrain: f32, radius: f32, offset: f32, phase_offset: f32) -> Knobs {
    Knobs {
        terrain: (terrain, mark()),
        radius: (radius, mark()),
        offset: (offset, mark()),
        phase_offset: (phase_offset, mark()),
    }
}

fn render(voice: &mut Voice, f0: f32, k: Knobs, state: &WaveTerrainState) -> (f32, f32) {
    let controls = voice.control(f0, k, state);
    voice.step(controls)
}

/// `seconds` of a tone at `hz` (negative runs backward) and fixed knobs, one
/// output.
fn tone(sample_rate: f32, hz: f32, settings: (f32, f32, f32), pm: bool, seconds: f32) -> Vec<f32> {
    let state = WaveTerrainState::new(sample_rate);
    let mut voice = Voice::default();
    (0..(sample_rate * seconds) as usize)
        .map(|_| {
            let (out, aux) = render(
                &mut voice,
                hz / sample_rate,
                knobs(settings.0, settings.1, settings.2),
                &state,
            );
            if pm { aux } else { out }
        })
        .collect()
}

/// Replays `wave_terrain.f32` (Plaits' path oscillator and terrains,
/// rendered a sample at a time): eight pitches, each from a fresh voice, with
/// the knobs sweeping all eight terrains.
#[test]
fn matches_plaits() {
    let state = WaveTerrainState::new(48000.0);
    let mut voice = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (n, record) in records("wave_terrain", 4 + 2).iter().enumerate() {
        if n % 2400 == 0 {
            voice = Voice::default();
        }
        let (out, pm) = render(
            &mut voice,
            record[0],
            knobs(record[1], record[2], record[3]),
            &state,
        );
        actual.extend([out, pm]);
        expected.extend([record[4], record[5]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// A steady tone has the same harmonics at 44.1 and 96 kHz as at 48 kHz.
/// Plaits' path oscillator traces an ellipse whose skew grows with its step
/// per sample, so the tone drifts with the rate by about 1% per octave above
/// c4; these pitches stay within 2%.
#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, settings) in [
        (-2.0, (0.5, 2.5, 2.5)),
        (-1.0, (3.0, 4.0, 1.0)),
        (0.0, (4.8, 3.0, 4.0)),
    ] {
        for pm in [false, true] {
            assert_rate_independent(
                |sample_rate| tone(sample_rate, voct_to_hz(pitch), settings, pm, 0.5),
                voct_to_hz(pitch),
                0.02,
            );
        }
    }
}

/// A pitch running backward traces the circle the other way: the same
/// harmonics as forward.
#[test]
fn runs_backward() {
    // A whole number of cycles in the measured window, so no harmonic leaks
    // into another.
    let hz = 200.0;
    for settings in [(1.0, 3.0, 2.0), (4.0, 4.0, 3.5)] {
        for pm in [false, true] {
            let forward = harmonics(&tone(48000.0, hz, settings, pm, 0.5), hz, 48000.0, 16);
            let backward = harmonics(&tone(48000.0, -hz, settings, pm, 0.5), hz, 48000.0, 16);
            let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
            for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
                assert!(
                    (b - f).abs() < 0.01 * loudest,
                    "{settings:?} pm {pm}, harmonic {}: {b} vs {f}",
                    h + 1
                );
            }
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = WaveTerrainState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..300 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 1.0,
        };
        let a = render(&mut held, 0.01, knobs(1.0, 1.0, 1.0), &state);
        let b = render(&mut glitched, 0.01, knobs(v, v, v), &state);
        assert_eq!(a, b, "sample {n}");
    }
}

/// Every knob setting, any pitch including NaN and infinite ones, at any
/// rate, leaves both outputs finite and within Plaits' full scale.
#[test]
fn stays_finite_and_bounded() {
    for sample_rate in [8000.0, 44100.0, 96000.0] {
        let state = WaveTerrainState::new(sample_rate);
        let mut voice = Voice::default();
        for n in 0..40000 {
            let f0 = match n % 500 {
                100 => f32::NAN,
                150 => f32::INFINITY,
                200 => f32::NEG_INFINITY,
                _ => (n % 11) as f32 * 0.05 - 0.25,
            };
            let t = n as f32 * 0.002;
            let phase_offset = match n % 13 {
                0 if n % 600 == 0 => f32::NAN,
                1 if n % 600 == 1 => f32::NEG_INFINITY,
                5 => -7.25,
                _ => (t * 3.5).sin() * 3.0,
            };
            let k = phase_knobs(
                2.5 + 3.0 * t.sin(),
                2.5 + 3.0 * (t * 1.3).sin(),
                2.5 + 3.0 * (t * 0.7).sin(),
                phase_offset,
            );
            let (out, pm) = render(&mut voice, f0, k, &state);
            assert!(
                out.is_finite() && pm.is_finite() && out.abs() <= 1.5 && pm.abs() <= 1.0,
                "{sample_rate} Hz, sample {n}: {out}, {pm}"
            );
        }
    }
}

/// A phase offset plays the same waveform shifted in time, forward or
/// backward and on both outputs: at a 100-sample period, a quarter-cycle
/// offset leads by 25 samples.
#[test]
fn phase_offset_shifts_the_waveform_in_time() {
    let state = WaveTerrainState::new(48000.0);
    for f0 in [0.01, -0.01] {
        for (terrain, radius, offset) in [(0.5, 2.5, 2.5), (3.0, 4.0, 1.0), (4.8, 5.0, 4.0)] {
            let tone = |phase_offset: f32| {
                let mut voice = Voice::default();
                (0..4000)
                    .map(|_| {
                        let k = phase_knobs(terrain, radius, offset, phase_offset);
                        render(&mut voice, f0, k, &state)
                    })
                    .collect::<Vec<(f32, f32)>>()
            };
            let ahead = tone(0.25);
            let plain = tone(0.0);
            // Backward, a lead of a quarter cycle is a lag of three.
            let lead = if f0 > 0.0 { 25 } else { 75 };
            for (output, pick) in [("height", 0), ("pm", 1)] {
                let pick = |samples: &[(f32, f32)]| {
                    samples
                        .iter()
                        .map(|&(a, b)| if pick == 0 { a } else { b })
                        .collect::<Vec<f32>>()
                };
                let (error, at) = max_error(
                    &pick(&ahead[1000..3000]),
                    &pick(&plain[1000 + lead..3000 + lead]),
                );
                assert!(
                    error < 1e-3,
                    "f0 {f0}, terrain {terrain}, {output}: error {error} at {at}"
                );
            }
        }
    }
}

/// An offset that moves, or jumps, ends where a voice started at it plays,
/// within the rounding its path gathers on the way.
#[test]
fn moving_phase_offset_lands_on_the_held_one() {
    let state = WaveTerrainState::new(48000.0);
    let mut held = Voice::default();
    let mut glided = Voice::default();
    let mut jumped = Voice::default();
    for n in 0..6000 {
        let glide = (n as f32 / 2000.0).min(1.0) * 0.4;
        let jump = if n < 100 { 0.9 } else { 0.4 };
        let a = render(&mut held, 0.01, phase_knobs(3.0, 4.0, 1.0, 0.4), &state);
        let b = render(&mut glided, 0.01, phase_knobs(3.0, 4.0, 1.0, glide), &state);
        let c = render(&mut jumped, 0.01, phase_knobs(3.0, 4.0, 1.0, jump), &state);
        if n >= 3000 {
            for (name, other) in [("glided", b), ("jumped", c)] {
                assert!(
                    (a.0 - other.0).abs() < 1e-3 && (a.1 - other.1).abs() < 1e-3,
                    "{name}, sample {n}: {a:?} vs {other:?}"
                );
            }
        }
    }
}

/// A NaN or infinite phase offset holds the path where it is.
#[test]
fn non_finite_phase_offset_holds() {
    let state = WaveTerrainState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..300 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 0.3,
        };
        let a = render(&mut held, 0.01, phase_knobs(1.0, 3.0, 2.0, 0.3), &state);
        let b = render(&mut glitched, 0.01, phase_knobs(1.0, 3.0, 2.0, v), &state);
        assert_eq!(a, b, "sample {n}");
    }
}
