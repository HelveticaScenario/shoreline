use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};

/// `seconds` of both sides at `hz` (negative runs backward), knobs held.
fn render(sample_rate: f32, hz: f32, knobs: [f32; 3], seconds: f32) -> Vec<(f32, f32)> {
    let state = StringMachineState::new(sample_rate);
    let mut lane = Lane::new(&state.rates);
    let f0 = hz / sample_rate;
    (0..(sample_rate * seconds) as usize)
        .map(|_| lane.step(f0, f0.abs(), knobs, &state))
        .collect()
}

fn side(rendered: &[(f32, f32)], right: bool) -> Vec<f32> {
    rendered
        .iter()
        .map(|&(l, r)| if right { r } else { l })
        .collect()
}

/// Replays `string_machine.f32`: Plaits' chord bank, divide-down
/// oscillators, filters and ensemble driven a sample at a time, with the
/// port's per-sample registration smoothing. It holds chords swept both ways,
/// the registration and tone swept, pitch steps and a sweep, a pitch high
/// enough to shift octaves and fade, registration jumps and a moving tone.
#[test]
fn matches_plaits() {
    let state = StringMachineState::new(48000.0);
    let mut lane = Lane::new(&state.rates);
    let mut actual = (Vec::new(), Vec::new());
    let mut expected = (Vec::new(), Vec::new());
    for record in records("string_machine", 4 + 2) {
        let (left, right) = lane.step(
            record[0],
            record[0],
            [record[1], record[2], record[3]],
            &state,
        );
        actual.0.push(left);
        actual.1.push(right);
        expected.0.push(record[4]);
        expected.1.push(record[5]);
    }
    for (name, actual, expected) in [
        ("left", &actual.0, &expected.0),
        ("right", &actual.1, &expected.1),
    ] {
        let (error, at) = max_error(actual, expected);
        assert!(error < 1e-5, "{name}: max error {error} at sample {at}");
    }
}

/// With the ensemble out (tone 2.5), a steady octave chord has the same
/// harmonics at 44.1 and 96 kHz as at 48 kHz, on both sides.
#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, registration) in [(-1.0, 0.0), (-2.0, 2.5), (0.0, 5.0)] {
        let hz = crate::dsp::utils::voct_to_hz(pitch);
        for right in [false, true] {
            assert_rate_independent(
                |sample_rate| {
                    side(
                        &render(sample_rate, hz, [0.0, registration, 2.5], 0.5),
                        right,
                    )
                },
                hz * 0.998,
                0.02,
            );
        }
    }
}

/// With the filter open, as it is from a low note up at the default tone,
/// a chord's top harmonics (8–18 kHz) are as loud at 44.1 and 96 kHz as at
/// 48 kHz, with the ensemble in. The saws' own band-limiting softens the
/// harmonics nearest each rate's Nyquist by up to about 1.4 dB.
#[test]
fn open_filter_is_open_at_any_rate() {
    let hz = 1000.0;
    for tone in [3.5, 5.0] {
        let levels = |sample_rate: f32| {
            let rendered = render(sample_rate, hz, [0.0, 0.0, tone], 0.5);
            harmonics(&side(&rendered, false), hz * 0.998, sample_rate, 18)
        };
        let reference = levels(48000.0);
        for sample_rate in [44100.0, 96000.0] {
            for (h, (a, r)) in levels(sample_rate)
                .iter()
                .zip(&reference)
                .enumerate()
                .skip(7)
            {
                let db = 20.0 * (a / r).log10();
                assert!(
                    db.abs() < 1.5,
                    "tone {tone}, {sample_rate} Hz, harmonic {}: {db} dB",
                    h + 1
                );
            }
        }
    }
}

/// The chord's notes alternate between the sides: in a minor chord the root
/// leads on the left and the minor third on the right.
#[test]
fn alternate_notes_lead_each_side() {
    let hz = 110.0;
    let rendered = render(48000.0, hz, [3.0, 0.0, 2.5], 0.5);
    let level = |right: bool, ratio: f32| {
        harmonics(&side(&rendered, right), hz * 0.998 * ratio, 48000.0, 1)[0]
    };
    let third = semitones_to_ratio(3.0);
    assert!(level(false, 1.0) > 1.5 * level(true, 1.0));
    assert!(level(true, third) > 1.5 * level(false, third));
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = StringMachineState::new(48000.0);
    let mut held = Lane::new(&state.rates);
    let mut glitched = Lane::new(&state.rates);
    for n in 0..600 {
        let v = match n % 200 {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 2.7,
        };
        for k in 0..3 {
            let mut knobs = [2.7; 3];
            let a = held.step(0.01, 0.01, knobs, &state);
            knobs[k] = v;
            let b = glitched.step(0.01, 0.01, knobs, &state);
            assert_eq!(a, b, "knob {k}, sample {n}");
        }
    }
}

/// Every chord, registration and tone at any pitch, including NaN, infinite
/// and backward ones, at any rate, leave both sides finite and within full
/// scale before the output's clip.
#[test]
fn stays_finite_and_bounded() {
    for sample_rate in [8000.0, 44100.0, 96000.0] {
        let state = StringMachineState::new(sample_rate);
        let mut lane = Lane::new(&state.rates);
        for n in 0..60000 {
            let f0 = match n % 700 {
                100 => f32::NAN,
                150 => f32::INFINITY,
                200 => f32::NEG_INFINITY,
                _ => (n % 13) as f32 * 0.02 - 0.06,
            };
            let t = n as f32;
            let knobs = [
                5.0 + 5.5 * (t * 0.0007).sin(),
                2.5 + 2.6 * (t * 0.0011).sin(),
                2.5 + 2.6 * (t * 0.0005).sin(),
            ];
            let (left, right) = lane.step(f0, f0, knobs, &state);
            for out in [left, right] {
                assert!(
                    out.is_finite() && (OUTPUT_GAIN * out).abs() <= FULL_SCALE,
                    "{sample_rate} Hz, sample {n}: {out}"
                );
            }
        }
    }
}
