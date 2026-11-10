use super::*;
use crate::dsp::mi::core::fixture::{max_error, records};

/// A channel at `sample_rate`, seeded as `particles.cc` seeds stmlib's
/// generator.
fn channel(state: &ParticlesState) -> Channel {
    Channel {
        random: Random::new(0x21),
        seeded: true,
        ..Channel::new(state)
    }
}

/// Replays `particles.f32` (Plaits' particle filters, post filter, diffuser
/// and limiter, with each block's impulses drawn in Plaits' order): dense
/// with the resonance sweeping through diffusion, density sweeping, and
/// sparse with syncs.
#[test]
fn matches_plaits() {
    let state = ParticlesState::new(48000.0);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for records in records("particles", 5 + 2).chunks(9600) {
        let mut ch = channel(&state);
        for record in records {
            ch.control([record[1], record[2], record[3]], record[4] > 0.5, &state);
            let (out, raw) = ch.step(record[0], &state);
            actual.extend([out, raw]);
            expected.extend([record[5], record[6]]);
        }
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// `seconds` of particles at `hz` with steady knobs: the output, and the sum
/// of every particle's impulses, which counts them (each averages 0.5) even
/// when several fire in one sample.
fn render(sample_rate: f32, hz: f32, knobs: [f32; 3], seconds: f32) -> (Vec<f32>, f32) {
    let state = ParticlesState::new(sample_rate);
    let mut ch = channel(&state);
    let mut impulses = 0.0;
    let out = (0..(sample_rate * seconds) as usize)
        .map(|_| {
            ch.control(knobs, false, &state);
            let (out, raw) = ch.step(hz / sample_rate, &state);
            impulses += raw;
            out
        })
        .collect();
    (out, impulses)
}

/// Particles fire as often, and ring as loud, at any rate: the impulse rate
/// within its random spread, the level within 1.5 dB.
#[test]
fn sounds_the_same_at_any_rate() {
    for knobs in [[3.5, 2.0, 4.0], [4.5, 3.0, 1.0], [4.0, 1.0, 2.5]] {
        let (reference, reference_impulses) = render(48000.0, 261.63, knobs, 4.0);
        for sample_rate in [2000.0, 44100.0, 96000.0] {
            let (out, impulses) = render(sample_rate, 261.63, knobs, 4.0);
            let ratio = impulses / reference_impulses;
            assert!(
                (0.85..1.15).contains(&ratio),
                "{knobs:?} at {sample_rate} Hz: {impulses} impulses vs {reference_impulses}"
            );
            if sample_rate > 40000.0 {
                let level = 20.0 * (rms(&out) / rms(&reference)).log10();
                assert!(
                    level.abs() < 1.5,
                    "{knobs:?} at {sample_rate} Hz: level {level} dB"
                );
            }
        }
    }
}

/// A sync fires every particle on its own sample.
#[test]
fn sync_fires_every_particle_on_its_sample() {
    let state = ParticlesState::new(48000.0);
    for offset in [0, 5, 11] {
        let mut ch = channel(&state);
        for n in 0..(1000 + offset) {
            let sync = n == 1000 + offset - 1;
            ch.control([0.0, 2.0, 2.5], sync, &state);
            let (_, raw) = ch.step(0.005, &state);
            if sync {
                assert!(raw > 5.99, "offset {offset}: raw {raw}");
            }
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = ParticlesState::new(48000.0);
    let mut held = channel(&state);
    let mut glitched = channel(&state);
    for n in 0..4000 {
        let v = match n {
            2000 => f32::NAN,
            2001 => f32::INFINITY,
            2002 => f32::NEG_INFINITY,
            _ => 3.0,
        };
        held.control([3.0; 3], false, &state);
        glitched.control([v; 3], false, &state);
        assert_eq!(
            held.step(0.01, &state),
            glitched.step(0.01, &state),
            "sample {n}"
        );
    }
}

/// Every knob setting, any pitch and syncs, at any rate, leave both outputs
/// finite. A sync's six impulses overshoot the slow limiter, as in Plaits;
/// the output stage clips them.
#[test]
fn stays_finite_and_bounded() {
    for sample_rate in [2000.0, 8000.0, 44100.0, 96000.0] {
        let state = ParticlesState::new(sample_rate);
        let mut ch = channel(&state);
        for n in 0..60000 {
            let f0 = match n % 500 {
                100 => f32::NAN,
                150 => f32::INFINITY,
                _ => (n % 13) as f32 * 0.04,
            };
            let f0 = limit_frequency(f0, 0.5).max(1.0 / sample_rate);
            let t = n as f32 * 0.002;
            let knobs = [
                2.5 + 3.0 * t.sin(),
                2.5 + 3.0 * (t * 1.3).sin(),
                2.5 + 3.0 * (t * 0.7).sin(),
            ];
            ch.control(knobs, n % 1777 == 0, &state);
            let (out, raw) = ch.step(f0, &state);
            assert!(
                out.is_finite() && raw.is_finite(),
                "{sample_rate} Hz, sample {n}: {out}, {raw}"
            );
        }
    }
}
