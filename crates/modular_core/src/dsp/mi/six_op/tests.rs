use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(bank: f32, patch: f32, brightness: f32, envelope: f32, accent: f32) -> Knobs {
    Knobs {
        bank,
        patch,
        brightness: (brightness, mark()),
        envelope: (envelope, mark()),
        accent: (accent, mark()),
    }
}

fn channel(seed: u32) -> Channel {
    Channel {
        random: Random::new(seed),
        ..Channel::default()
    }
}

/// Replays `six_op.f32` (Plaits' six-op engine with per-sample controls):
/// gated notes across all three banks, feedback loops spanning several
/// operators, fixed-frequency operators, pitch envelopes and LFOs, notes
/// alternating between patches, and scrubbed drones.
#[test]
fn matches_plaits() {
    let state = SixOpState::new(48000.0);
    let mut current = -1.0;
    let mut ch = channel(0);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("six_op", 9) {
        if record[0] != current {
            current = record[0];
            ch = channel(0x5eed + current as u32);
        }
        let gate = match record[5] {
            g if g < 0.0 => None,
            g => Some(g > 0.0),
        };
        let out = ch.step(
            record[1],
            knobs(record[6], record[7], record[2], record[3], record[4]),
            gate,
            &state,
        );
        actual.push(out);
        expected.push(record[8]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {at}");
}

/// Plaits has a renderer for every chain of every algorithm.
#[test]
fn every_algorithm_compiles() {
    for algorithm in 0..32 {
        algorithms::Algorithm::compile(algorithm);
    }
}

/// `seconds` of a note at `hz` on `bank`/`patch`, at `sample_rate`: held for
/// the first half when `gated`, otherwise scrubbed at `envelope`.
fn render(
    sample_rate: f32,
    hz: f32,
    bank: f32,
    patch: f32,
    gated: bool,
    envelope: f32,
    seconds: f32,
) -> Vec<f32> {
    let state = SixOpState::new(sample_rate);
    let mut ch = channel(1);
    let samples = (sample_rate * seconds) as usize;
    (0..samples)
        .map(|n| {
            let gate = gated.then_some(n < samples / 2);
            ch.step(
                hz / sample_rate,
                knobs(bank, patch, 2.5, envelope, 4.0),
                gate,
                &state,
            )
        })
        .collect()
}

/// A scrubbed drone has the same harmonics at 44.1 and 96 kHz as at 48 kHz.
/// Operator feedback reads the last two samples, so the strongest feedback
/// sounds a little different at other rates.
#[test]
fn sounds_the_same_at_any_rate() {
    for (bank, patch, envelope, tolerance) in [
        (0.0, 2.0, 2.5, 0.02),
        (2.0, 1.0, 2.5, 0.02),
        (1.0, 0.0, 2.5, 0.02),
        (0.0, 13.0, 3.0, 0.02),
        (2.0, 29.0, 2.0, 0.05),
    ] {
        let hz = voct_to_hz(-1.0);
        assert_rate_independent(
            |sample_rate| render(sample_rate, hz, bank, patch, false, envelope, 0.5),
            hz,
            tolerance,
        );
    }
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// A played note's envelopes take the same time at any rate: its level in
/// 20 ms windows through attack, decay and release.
#[test]
fn envelopes_take_the_same_time_at_any_rate() {
    let windows = |sample_rate: f32, bank: f32, patch: f32, envelope: f32| {
        let out = render(sample_rate, 220.0, bank, patch, true, envelope, 1.0);
        let window = (0.02 * sample_rate) as usize;
        out.chunks_exact(window).map(rms).collect::<Vec<_>>()
    };
    for (bank, patch, envelope) in [(1.0, 0.0, 2.5), (0.0, 0.0, 1.0), (1.0, 20.0, 4.0)] {
        let reference = windows(48000.0, bank, patch, envelope);
        let loudest = reference.iter().fold(0.0f32, |m, &a| m.max(a));
        for sample_rate in [44100.0, 96000.0] {
            let actual = windows(sample_rate, bank, patch, envelope);
            for (w, (a, r)) in actual.iter().zip(&reference).enumerate() {
                if *r > 0.01 * loudest {
                    let db = 20.0 * (a / r).log10();
                    assert!(
                        db.abs() < 1.0,
                        "bank {bank} patch {patch}, {sample_rate} Hz, window {w}: {db} dB"
                    );
                }
            }
        }
    }
}

/// Through-zero FM runs the operators backward: without feedback, a negative
/// frequency plays the forward waveform inverted.
#[test]
fn runs_backward() {
    let state = SixOpState::new(48000.0);
    // Patches without feedback or fixed-frequency operators.
    for (bank, patch) in [(0.0, 2.0), (2.0, 1.0)] {
        let render = |f0: f32| {
            let mut ch = channel(1);
            (0..4800)
                .map(|_| ch.step(f0, knobs(bank, patch, 2.5, 2.5, 4.0), None, &state))
                .collect::<Vec<_>>()
        };
        let forward = render(0.004);
        let backward = render(-0.004);
        let peak = forward.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let error = forward
            .iter()
            .zip(&backward)
            .fold(0.0f32, |m, (a, b)| m.max((a + b).abs()));
        assert!(peak > 0.1, "bank {bank} patch {patch}: silent");
        assert!(
            error < 1e-3 * peak,
            "bank {bank} patch {patch}: mirror error {error}"
        );
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = SixOpState::new(48000.0);
    let mut held = channel(7);
    let mut glitched = channel(7);
    for n in 0..4000 {
        let v = match n {
            500 => f32::NAN,
            501 => f32::INFINITY,
            502 => f32::NEG_INFINITY,
            _ => 2.0,
        };
        let gate = Some(n % 1000 < 700);
        let a = held.step(0.005, knobs(2.0, 2.0, 2.0, 2.0, 2.0), gate, &state);
        let b = glitched.step(0.005, knobs(v, v, v, v, v), gate, &state);
        assert_eq!(a, b, "sample {n}");
    }
}

/// Every patch stays finite and within full scale at extreme pitches, knob
/// jumps and any engine rate, including a NaN or infinite pitch.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 22050.0, 192000.0] {
        let state = SixOpState::new(sample_rate);
        for bank in 0..3 {
            for patch in 0..32 {
                let mut ch = channel(3);
                for n in 0..3000 {
                    let f0 = match n % 7 {
                        0 => 0.5,
                        1 => -0.5,
                        2 if n % 500 == 2 => f32::NAN,
                        3 if n % 500 == 3 => f32::INFINITY,
                        _ => 0.0005 + (n as f32 * 0.001).sin() * 0.05,
                    };
                    let knob = (n % 11) as f32 * 0.5;
                    let gate = (bank != 2).then_some(n % 400 < 250);
                    let out = ch.step(
                        f0,
                        knobs(bank as f32, patch as f32, knob, 5.0 - knob, 5.0),
                        gate,
                        &state,
                    );
                    assert!(
                        out.is_finite() && out.abs() <= 1.0,
                        "{sample_rate} Hz, bank {bank} patch {patch}, sample {n}: {out}"
                    );
                }
            }
        }
    }
}
