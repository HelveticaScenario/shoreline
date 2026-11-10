use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::utils::voct_to_hz;

fn knobs(centroid: f32, slope: f32, bumps: f32) -> [f32; 3] {
    [centroid, slope, bumps]
}

fn render(voice: &mut Voice, f0: f32, k: [f32; 3], series: Series, state: &AdditiveState) -> f32 {
    voice.control(k, state);
    voice.step(f0, series, state)
}

/// `seconds` of a tone at `hz` and fixed knobs.
fn tone(
    sample_rate: f32,
    hz: f32,
    (centroid, slope, bumps): (f32, f32, f32),
    series: Series,
    seconds: f32,
) -> Vec<f32> {
    let state = AdditiveState::new(sample_rate);
    let mut voice = Voice::default();
    (0..(sample_rate * seconds) as usize)
        .map(|_| {
            render(
                &mut voice,
                hz / sample_rate,
                knobs(centroid, slope, bumps),
                series,
                &state,
            )
        })
        .collect()
}

/// Replays `additive.f32` (Plaits' amplitude update and harmonic
/// oscillators, rendered a sample at a time) for both series.
#[test]
fn matches_plaits() {
    let state = AdditiveState::new(48000.0);
    let mut integer = Voice::default();
    let mut organ = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("additive", 4 + 2) {
        let k = || knobs(record[1], record[2], record[3]);
        actual.extend([
            render(&mut integer, record[0], k(), Series::Integer, &state),
            render(&mut organ, record[0], k(), Series::Organ, &state),
        ]);
        expected.extend([record[4], record[5]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// A steady tone has the same harmonics at 44.1 and 96 kHz as at 48 kHz.
#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, settings) in [(-1.0, (2.5, 2.5, 0.0)), (0.5, (1.0, 1.0, 3.0))] {
        for series in [Series::Integer, Series::Organ] {
            assert_rate_independent(
                |sample_rate| tone(sample_rate, voct_to_hz(pitch), settings, series, 0.5),
                voct_to_hz(pitch),
                0.02,
            );
        }
    }
}

/// After the centroid jumps, the spectrum moves toward it at the same speed
/// at any rate.
#[test]
fn glides_at_the_same_speed_at_any_rate() {
    let hz = voct_to_hz(-1.0);
    let glide = |sample_rate: f32| {
        let state = AdditiveState::new(sample_rate);
        let mut voice = Voice::default();
        let jump = (sample_rate * 0.2) as usize;
        let out: Vec<f32> = (0..(sample_rate * 0.26) as usize)
            .map(|n| {
                let centroid = if n < jump { 0.5 } else { 4.5 };
                render(
                    &mut voice,
                    hz / sample_rate,
                    knobs(centroid, 3.0, 0.0),
                    Series::Integer,
                    &state,
                )
            })
            .collect();
        // The second half of this window, 30–60 ms after the jump.
        harmonics(&out[jump..], hz, sample_rate, 16)
    };
    let reference = glide(48000.0);
    let loudest = reference.iter().fold(0.0f32, |m, &a| m.max(a));
    for sample_rate in [44100.0, 96000.0] {
        for (h, (a, r)) in glide(sample_rate).iter().zip(&reference).enumerate() {
            assert!(
                (a - r).abs() < 0.03 * loudest,
                "{sample_rate} Hz, harmonic {}: {a} vs {r}",
                h + 1
            );
        }
    }
}

/// A pitch running backward plays the same harmonics as forward.
#[test]
fn runs_backward() {
    let sample_rate = 48000.0;
    // A whole number of cycles in the measured window, so no harmonic leaks
    // into another.
    let hz = 200.0;
    for series in [Series::Integer, Series::Organ] {
        let forward = tone(sample_rate, hz, (2.0, 2.0, 2.0), series, 0.5);
        let backward = tone(sample_rate, -hz, (2.0, 2.0, 2.0), series, 0.5);
        let forward = harmonics(&forward, hz, sample_rate, 24);
        let backward = harmonics(&backward, hz, sample_rate, 24);
        for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
            assert!((b - f).abs() < 1e-3, "harmonic {}: {b} vs {f}", h + 1);
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = AdditiveState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..2000 {
        let v = match n {
            1000 => f32::NAN,
            1001 => f32::INFINITY,
            1002 => f32::NEG_INFINITY,
            _ => 1.0,
        };
        let a = render(
            &mut held,
            0.01,
            knobs(1.0, 1.0, 1.0),
            Series::Integer,
            &state,
        );
        let b = render(&mut glitched, 0.01, knobs(v, v, v), Series::Integer, &state);
        assert_eq!(a, b, "sample {n}");
    }
}

/// A NaN or infinite pitch, and every knob setting at any rate, leave the
/// output finite and within Plaits' full scale.
#[test]
fn stays_finite_and_bounded() {
    for sample_rate in [2000.0, 8000.0, 44100.0, 96000.0] {
        let state = AdditiveState::new(sample_rate);
        for series in [Series::Integer, Series::Organ] {
            let mut voice = Voice::default();
            for n in 0..20000 {
                let f0 = match n % 500 {
                    100 => f32::NAN,
                    150 => f32::INFINITY,
                    200 => f32::NEG_INFINITY,
                    _ => (n % 7) as f32 * 0.01 - 0.03,
                };
                let t = n as f32 * 0.003;
                let k = knobs(
                    2.5 + 3.0 * t.sin(),
                    2.5 + 3.0 * (t * 1.3).sin(),
                    2.5 + 3.0 * (t * 0.7).sin(),
                );
                let out = render(&mut voice, f0, k, series, &state);
                assert!(
                    out.is_finite() && out.abs() <= 1.0,
                    "{sample_rate} Hz, sample {n}: {out}"
                );
            }
        }
    }
}
