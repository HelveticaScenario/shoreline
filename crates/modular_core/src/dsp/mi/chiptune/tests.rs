use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, max_error, records};
use crate::dsp::utils::voct_to_hz;

/// The seed `chiptune.cc` starts stmlib's random generator from.
const FIXTURE_SEED: u32 = 0x1234567;

/// `seconds` of the chord and bass at `hz`, knobs held, struck every
/// `period` samples or with the trigger unpatched.
fn render(
    sample_rate: f32,
    hz: f32,
    knobs: Knobs,
    period: Option<usize>,
    seconds: f32,
) -> Vec<(f32, f32)> {
    let state = MiChiptuneState::new(sample_rate);
    let mut lane = Lane::default();
    (0..(sample_rate * seconds) as usize)
        .map(|n| {
            let clock = match period {
                Some(period) => Clock::Patched {
                    rising: n % period == 0,
                },
                None => Clock::Unpatched,
            };
            lane.step(hz / sample_rate, knobs, clock, false, &state)
        })
        .collect()
}

/// Replays `chiptune.f32`: Plaits' chord bank, arpeggiator, super square
/// voices and NES triangle driven a sample at a time. It holds chords swept,
/// the voicing and the shape swept, arpeggios up, up and down and at random,
/// with and without the bass decaying, patterns and chords changing between
/// triggers, high pitches at the voices' limit and a fast four-octave
/// arpeggio.
#[test]
fn matches_plaits() {
    let state = MiChiptuneState::new(48000.0);
    let mut lane = Lane {
        random: Random::new(FIXTURE_SEED),
        ..Lane::default()
    };
    let mut actual = (Vec::new(), Vec::new());
    let mut expected = (Vec::new(), Vec::new());
    for record in records("chiptune", 8 + 2) {
        let clock = match record[7] {
            t if t < 0.0 => Clock::Unpatched,
            t => Clock::Patched { rising: t > 0.0 },
        };
        let knobs = [record[1], record[2], record[3], record[4], record[5]];
        let (chord, bass) = lane.step(record[0], knobs, clock, record[6] > 0.0, &state);
        actual.0.push(chord);
        actual.1.push(bass);
        expected.0.push(record[8]);
        expected.1.push(record[9]);
    }
    for (name, actual, expected) in [
        ("chord", &actual.0, &expected.0),
        ("bass", &actual.1, &expected.1),
    ] {
        let (error, at) = max_error(actual, expected);
        assert!(error < 1e-5, "{name}: max error {error} at sample {at}");
    }
}

/// A steady chord and its bass have the same harmonics at 44.1 and 96 kHz as
/// at 48 kHz.
#[test]
fn sounds_the_same_at_any_rate() {
    // The octave chord, lowest voicing: its notes sit on the harmonics of
    // two octaves below `freq`.
    for shape in [1.0, 2.5, 4.0] {
        let hz = voct_to_hz(0.0);
        assert_rate_independent(
            |sample_rate| {
                render(sample_rate, hz, [0.0, 0.0, 0.0, shape, 5.0], None, 0.5)
                    .into_iter()
                    .map(|(chord, _)| chord)
                    .collect()
            },
            hz * 0.25,
            0.02,
        );
    }
    let hz = voct_to_hz(-1.0);
    assert_rate_independent(
        |sample_rate| {
            render(sample_rate, hz, [10.0, 2.5, 0.0, 2.5, 5.0], None, 0.5)
                .into_iter()
                .map(|(_, bass)| bass)
                .collect()
        },
        hz * 0.5,
        0.02,
    );
}

/// The arpeggio's notes fade as fast at any rate: the level halfway between
/// triggers is the same share of the level at the trigger.
#[test]
fn decays_as_fast_at_any_rate() {
    let fade = |sample_rate: f32| {
        let period = (sample_rate * 0.25) as usize;
        let out: Vec<f32> = render(
            sample_rate,
            220.0,
            [10.0, 2.5, 0.0, 2.5, 2.0],
            Some(period),
            1.0,
        )
        .into_iter()
        .map(|(chord, _)| chord.abs())
        .collect();
        let peak = |from: usize| {
            out[from..from + period / 10]
                .iter()
                .fold(0.0f32, |m, &v| m.max(v))
        };
        peak(period + period / 2) / peak(period)
    };
    let reference = fade(48000.0);
    assert!(reference < 0.5, "decays: {reference}");
    for sample_rate in [44100.0, 96000.0] {
        let ratio = fade(sample_rate);
        assert!(
            (ratio - reference).abs() < 0.02,
            "{sample_rate} Hz: {ratio} vs {reference}"
        );
    }
}

/// With `decay` at 5 the arpeggio holds each note until the next trigger,
/// and the bass never fades, decaying or not.
#[test]
fn full_decay_holds() {
    let state = MiChiptuneState::new(48000.0);
    for bass_decay in [false, true] {
        let mut lane = Lane::default();
        let mut levels = Vec::new();
        for n in 0..48000 {
            let clock = Clock::Patched {
                rising: n % 12000 == 0,
            };
            let (chord, _) = lane.step(
                220.0 / 48000.0,
                [10.0, 2.5, 0.0, 2.5, 5.0],
                clock,
                bass_decay,
                &state,
            );
            levels.push(chord.abs());
        }
        assert_eq!(lane.envelope, 1.0);
        assert_eq!(lane.bass_envelope, 0.0);
        let late = levels[11000..12000].iter().fold(0.0f32, |m, &v| m.max(v));
        assert!(late > 0.9, "bass decay {bass_decay}: {late}");
    }
}

/// Each trigger steps the arpeggio to the chord's next note, and the bass
/// follows its octave.
#[test]
fn steps_through_the_chord() {
    let state = MiChiptuneState::new(48000.0);
    let mut lane = Lane::default();
    let mut notes = Vec::new();
    for n in 0..8 * 100 {
        let clock = Clock::Patched {
            rising: n % 100 == 0,
        };
        lane.step(0.001, [9.0, 0.0, 1.0, 2.5, 5.0], clock, false, &state);
        if n % 100 == 0 {
            notes.push((lane.arpeggiator.note(), lane.arpeggiator.octave()));
        }
    }
    assert_eq!(
        notes,
        [
            (1, 0),
            (2, 0),
            (3, 0),
            (0, 1),
            (1, 1),
            (2, 1),
            (3, 1),
            (0, 0)
        ]
    );
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = MiChiptuneState::new(48000.0);
    for period in [None, Some(150)] {
        let mut held = Lane::default();
        let mut glitched = Lane::default();
        for n in 0..1200 {
            let v = match n % 200 {
                100 => f32::NAN,
                101 => f32::INFINITY,
                102 => f32::NEG_INFINITY,
                _ => 2.7,
            };
            let clock = match period {
                Some(period) => Clock::Patched {
                    rising: n % period == 0,
                },
                None => Clock::Unpatched,
            };
            for k in 0..5 {
                let mut knobs = [2.7; 5];
                let a = held.step(0.01, knobs, clock, true, &state);
                knobs[k] = v;
                let b = glitched.step(0.01, knobs, clock, true, &state);
                assert_eq!(a, b, "{period:?}: knob {k}, sample {n}");
            }
        }
    }
}

/// Every chord, voicing, pattern, shape and decay at any pitch, including
/// NaN, infinite and backward ones, at any rate, leave both outputs finite
/// and within full scale, clocked or not.
#[test]
fn stays_finite_and_bounded() {
    for sample_rate in [8000.0, 44100.0, 96000.0] {
        let state = MiChiptuneState::new(sample_rate);
        let mut lane = Lane::default();
        for n in 0..60000 {
            let f0 = match n % 700 {
                100 => f32::NAN,
                150 => f32::INFINITY,
                200 => f32::NEG_INFINITY,
                _ => (n % 13) as f32 * 0.04 - 0.12,
            };
            let t = n as f32;
            let knobs = [
                5.0 + 5.5 * (t * 0.0007).sin(),
                2.5 + 2.6 * (t * 0.0011).sin(),
                5.5 + 6.0 * (t * 0.0003).sin(),
                2.5 + 2.6 * (t * 0.0005).sin(),
                2.5 + 2.6 * (t * 0.0009).sin(),
            ];
            let clock = if (n / 20000) % 2 == 0 {
                Clock::Unpatched
            } else {
                Clock::Patched {
                    rising: n % 97 == 0,
                }
            };
            let (chord, bass) = lane.step(f0, knobs, clock, n % 3 == 0, &state);
            for out in [chord, bass] {
                assert!(
                    out.is_finite() && (OUTPUT_GAIN * out).abs() <= 5.5,
                    "{sample_rate} Hz, sample {n}: {out}"
                );
            }
        }
    }
}

/// Through-zero FM below zero plays the same tone as the same pitch above.
#[test]
fn reflects_backward_pitch() {
    let knobs = [10.0, 2.5, 0.0, 2.5, 5.0];
    let forward = render(48000.0, 200.0, knobs, None, 0.25);
    let backward = render(48000.0, -200.0, knobs, None, 0.25);
    assert_eq!(forward, backward);
}
