use super::*;
use crate::dsp::mi::core::fixture::{harmonics, max_error, records};

/// A channel seeded as `swarm.cc` seeds stmlib's generator.
fn channel() -> Channel {
    Channel {
        random: Random::new(0x21),
        seeded: true,
        ..Channel::default()
    }
}

/// Replays `swarm.f32` (Plaits' grain envelopes, saws, sines and limiter,
/// rendered a sample at a time): the saws in every case, and the sines in the
/// first, at one pitch with the voices in unison. A moving pitch keeps the
/// sines' radius, where Plaits' sine drifts.
#[test]
fn matches_plaits() {
    let state = SwarmState::new(48000.0);
    let (mut saw_actual, mut saw_expected) = (Vec::new(), Vec::new());
    let (mut sine_actual, mut sine_expected) = (Vec::new(), Vec::new());
    for (case, records) in records("swarm", 5 + 2).chunks(9600).enumerate() {
        let mut ch = channel();
        let burst_mode = case == 3;
        for record in records {
            ch.control(
                [record[1], record[2], record[3]],
                burst_mode,
                record[4] > 0.5,
                &state,
            );
            let (saws, sines) = ch.step(record[0], &state);
            saw_actual.push(saws);
            saw_expected.push(record[5]);
            if case == 0 {
                sine_actual.push(sines);
                sine_expected.push(record[6]);
            }
        }
    }
    let (error, at) = max_error(&saw_actual, &saw_expected);
    assert!(error < 1e-4, "saws: max error {error} at sample {at}");
    let (error, at) = max_error(&sine_actual, &sine_expected);
    assert!(error < 1e-5, "sines: max error {error} at sample {at}");
}

/// `seconds` of a swarm at `hz` (negative runs backward) with steady knobs:
/// the saws and the sines.
fn swarm(sample_rate: f32, hz: f32, knobs: [f32; 3], seconds: f32) -> (Vec<f32>, Vec<f32>) {
    let state = SwarmState::new(sample_rate);
    let mut ch = channel();
    (0..(sample_rate * seconds) as usize)
        .map(|_| {
            ch.control(knobs, false, false, &state);
            ch.step(hz / sample_rate, &state)
        })
        .unzip()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// A swarm in unison sounds the same at 44.1 and 96 kHz as at 48 kHz, and a
/// spread swarm reaches the same level: its grains run on a timebase in
/// seconds.
#[test]
fn sounds_the_same_at_any_rate() {
    let hz = 130.81;
    let reference = swarm(48000.0, hz, [2.5, 0.0, 2.0], 1.0);
    let spread_reference = swarm(48000.0, hz, [3.0, 3.0, 1.0], 4.0);
    for sample_rate in [44100.0, 96000.0] {
        let (saws, sines) = swarm(sample_rate, hz, [2.5, 0.0, 2.0], 1.0);
        for (actual, expected) in [(&saws, &reference.0), (&sines, &reference.1)] {
            let a = harmonics(actual, hz, sample_rate, 8);
            let e = harmonics(expected, hz, 48000.0, 8);
            let loudest = e.iter().fold(0.0f32, |m, &v| m.max(v));
            for (h, (a, e)) in a.iter().zip(&e).enumerate() {
                assert!(
                    (a - e).abs() < 0.03 * loudest,
                    "{sample_rate} Hz, harmonic {}: {a} vs {e}",
                    h + 1
                );
            }
        }
        let (saws, sines) = swarm(sample_rate, hz, [3.0, 3.0, 1.0], 4.0);
        for (actual, expected) in [(&saws, &spread_reference.0), (&sines, &spread_reference.1)] {
            let level = 20.0 * (rms(actual) / rms(expected)).log10();
            assert!(
                level.abs() < 1.0,
                "{sample_rate} Hz: spread level {level} dB"
            );
        }
    }
}

/// A pitch running backward plays the same harmonics as forward.
#[test]
fn runs_backward() {
    // A whole number of cycles in the measured window.
    let hz = 200.0;
    let forward = swarm(48000.0, hz, [2.5, 0.0, 4.5], 0.5);
    let backward = swarm(48000.0, -hz, [2.5, 0.0, 4.5], 0.5);
    for (b, f) in [(&backward.0, &forward.0), (&backward.1, &forward.1)] {
        let b = harmonics(b, hz, 48000.0, 12);
        let f = harmonics(f, hz, 48000.0, 12);
        let loudest = f.iter().fold(0.0f32, |m, &v| m.max(v));
        for (h, (b, f)) in b.iter().zip(&f).enumerate() {
            assert!(
                (b - f).abs() < 0.01 * loudest,
                "harmonic {}: {b} vs {f}",
                h + 1
            );
        }
    }
}

/// A burst starts on its own sample: every voice's grain restarts there,
/// whatever the tick.
#[test]
fn bursts_start_on_their_sample() {
    let state = SwarmState::new(48000.0);
    for offset in [0, 5, 11] {
        let mut ch = channel();
        for n in 0..(1000 + offset) {
            ch.control([2.5, 2.0, 2.0], true, n == 1000 + offset - 1, &state);
            ch.step(0.003, &state);
        }
        for voice in &ch.voices {
            assert_eq!(voice.envelope_phase(), 0.5, "offset {offset}");
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = SwarmState::new(48000.0);
    let mut held = channel();
    let mut glitched = channel();
    for n in 0..2000 {
        let v = match n {
            1000 => f32::NAN,
            1001 => f32::INFINITY,
            1002 => f32::NEG_INFINITY,
            _ => 1.0,
        };
        held.control([1.0; 3], false, false, &state);
        glitched.control([v; 3], false, false, &state);
        assert_eq!(
            held.step(0.01, &state),
            glitched.step(0.01, &state),
            "sample {n}"
        );
    }
}

/// Every knob setting, any pitch, bursts, at any rate, leave both outputs
/// finite.
#[test]
fn stays_finite() {
    for sample_rate in [2000.0, 8000.0, 44100.0, 96000.0] {
        let state = SwarmState::new(sample_rate);
        let mut ch = channel();
        for n in 0..40000 {
            let f0 = match n % 500 {
                100 => f32::NAN,
                150 => f32::INFINITY,
                200 => f32::NEG_INFINITY,
                _ => (n % 11) as f32 * 0.05 - 0.25,
            };
            let f0 = limit_frequency(f0, 0.5);
            let t = n as f32 * 0.002;
            let knobs = [
                2.5 + 3.0 * t.sin(),
                2.5 + 3.0 * (t * 1.3).sin(),
                2.5 + 3.0 * (t * 0.7).sin(),
            ];
            ch.control(knobs, n % 2 == 0, n % 977 == 0, &state);
            let (saws, sines) = ch.step(f0, &state);
            assert!(
                saws.is_finite() && sines.is_finite(),
                "{sample_rate} Hz, sample {n}"
            );
        }
    }
}

/// Grains run on a timebase in seconds: after a second, every voice's grain
/// is where it is at 48 kHz, including at rates under 4 kHz, where several
/// ticks share a sample.
#[test]
fn grains_keep_time_at_any_rate() {
    let phases = |sample_rate: f32| {
        let state = SwarmState::new(sample_rate);
        let mut ch = channel();
        // Through t = 1 s, so every rate has stepped the same 4001 ticks.
        for _ in 0..=sample_rate as usize {
            ch.control([3.5, 2.0, 2.0], false, false, &state);
            ch.step(0.001, &state);
        }
        ch.voices.map(|v| v.envelope_phase())
    };
    let reference = phases(48000.0);
    for sample_rate in [2000.0, 3000.0, 8000.0, 96000.0] {
        let actual = phases(sample_rate);
        for (v, (a, r)) in actual.iter().zip(&reference).enumerate() {
            assert!(
                (a - r).abs() < 1e-4,
                "{sample_rate} Hz, voice {v}: {a} vs {r}"
            );
        }
    }
}

/// A backward phase that lands a hair below 0 and then stops keeps the saw
/// finite.
#[test]
fn saw_survives_stopping_on_a_wrap() {
    let state = SwarmState::new(48000.0);
    let mut ch = channel();
    for f0 in [-0.25, -0.25, -0.25, -0.125, -0.125_000_01, 0.0] {
        ch.control([2.5, 0.0, 2.5], false, false, &state);
        ch.step(f0, &state);
    }
    for n in 0..48000 {
        ch.control([2.5, 0.0, 2.5], false, false, &state);
        let (saws, sines) = ch.step(0.0, &state);
        assert!(saws.is_finite() && sines.is_finite(), "sample {n}");
    }
}
