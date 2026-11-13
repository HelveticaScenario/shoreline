mod knobs;

use super::analog::{Knobs as SnareKnobs, Lane as SnareLane, SnareState};
use super::synthetic::{Knobs as SynthKnobs, Lane as SynthLane, SynthSnareState};
use crate::dsp::mi::core::fixture::{max_error, records};
use crate::dsp::mi::drum::measure::{
    assert_rate_independent, mark, peak, pitch_cents, rms, window,
};
use crate::dsp::mi::drum::pitch;
use crate::dsp::oscillators::FmMode;

/// `$mi.snare`'s knobs in volts: tone, decay, snappy and accent.
fn snare_knobs([tone, decay, snappy, accent]: [f32; 4]) -> SnareKnobs {
    SnareKnobs {
        tone: (tone, mark()),
        decay: (decay, mark()),
        snappy: (snappy, mark()),
        accent: (accent, mark()),
    }
}

/// `$mi.synthSnare`'s knobs in volts: attack FM, decay, snappy and accent.
fn synth_knobs([attack_fm, decay, snappy, accent]: [f32; 4]) -> SynthKnobs {
    SynthKnobs {
        attack_fm: (attack_fm, mark()),
        decay: (decay, mark()),
        snappy: (snappy, mark()),
        accent: (accent, mark()),
    }
}

const SR: f32 = 48000.0;

/// Replays `snare.f32` (Plaits' two snare drums rendered a sample at a
/// time): strikes across accents and every knob's range, the tone across its
/// 2/3 split, retriggers mid-decay and at audio rate, a pitch swept by FM,
/// high and near-1 Hz pitches, a drone with the trigger unpatched, strikes
/// again, and a hit ringing out through the synthetic drum's quiet tail.
#[test]
fn matches_plaits() {
    let snare_state = SnareState::new(SR);
    let synth_state = SynthSnareState::new(SR);
    let mut snare = SnareLane::new(0x1234567);
    let mut synth = SynthLane::new(0x7654321);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for r in records("snare", 8 + 2) {
        let trigger = (r[6] == 0.0).then_some(if r[7] != 0.0 { 5.0 } else { 0.0 });
        actual.push(snare.step(
            r[0],
            snare_knobs([r[1], r[2], r[3], r[4]]),
            trigger,
            &snare_state,
        ));
        actual.push(synth.step(
            r[0],
            synth_knobs([r[5], r[2], r[3], r[4]]),
            trigger,
            &synth_state,
        ));
        expected.extend([r[8], r[9]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(
        error < 1e-5,
        "max error {error} at sample {} ({})",
        at / 2,
        ["snare", "synthSnare"][at % 2]
    );
}

/// `seconds` of a drum `step` renders at `sample_rate`, struck once after its
/// noise filters settle from a fresh start.
fn hit(sample_rate: f32, seconds: f32, mut step: impl FnMut(Option<f32>) -> f32) -> Vec<f32> {
    let settle = (0.2 * sample_rate) as usize;
    (0..settle + (seconds * sample_rate) as usize)
        .map(|n| step(Some(if n == settle { 5.0 } else { 0.0 })))
        .skip(settle)
        .collect()
}

/// `seconds` of `$mi.snare` at `hz` with noise from `seed`, struck once.
fn snare_hit(sample_rate: f32, hz: f32, knobs: [f32; 4], seed: u32, seconds: f32) -> Vec<f32> {
    let state = SnareState::new(sample_rate);
    let mut lane = SnareLane::new(seed);
    hit(sample_rate, seconds, |trigger| {
        lane.step(hz / sample_rate, snare_knobs(knobs), trigger, &state)
    })
}

/// `seconds` of `$mi.synthSnare` at `hz` with noise from `seed`, struck once.
fn synth_hit(sample_rate: f32, hz: f32, knobs: [f32; 4], seed: u32, seconds: f32) -> Vec<f32> {
    let state = SynthSnareState::new(sample_rate);
    let mut lane = SynthLane::new(seed);
    hit(sample_rate, seconds, |trigger| {
        lane.step(hz / sample_rate, synth_knobs(knobs), trigger, &state)
    })
}

const RATES: [f32; 3] = [44100.0, 96000.0, 192000.0];

/// The default knobs with no snares.
const NO_SNARES: [f32; 4] = [2.5, 2.5, 0.0, 4.0];

/// With no snares, a strike has the same pitch, level and decay at any rate.
#[test]
fn shells_sound_the_same_at_any_rate() {
    for (hz, knobs) in [
        (180.0, [2.5, 2.5, 0.0, 4.0]),
        (250.0, [4.5, 4.0, 0.0, 5.0]),
        (120.0, [1.0, 1.0, 0.0, 2.0]),
    ] {
        assert_rate_independent("snare", hz, &RATES, |sample_rate| {
            snare_hit(sample_rate, hz, knobs, 1, 0.3)
        });
        assert_rate_independent("synthSnare", hz, &RATES, |sample_rate| {
            synth_hit(sample_rate, hz, knobs, 1, 0.3)
        });
    }
}

/// `x` at `sample_rate` through two one-poles at 8 kHz, which leave out what
/// lies above 48 kHz's band.
fn audible(x: &[f32], sample_rate: f32) -> Vec<f32> {
    let c = 1.0 - (-std::f32::consts::TAU * 8000.0 / sample_rate).exp();
    let (mut a, mut b) = (0.0, 0.0);
    x.iter()
        .map(|&v| {
            a += c * (v - a);
            b += c * (a - b);
            b
        })
        .collect()
}

/// The snares of a strike are as loud, and fade as fast, at any rate, over
/// many noise draws.
#[test]
fn snares_sound_the_same_at_any_rate() {
    type Render = fn(f32, f32, [f32; 4], u32, f32) -> Vec<f32>;
    for (name, render) in [("snare", snare_hit as Render), ("synthSnare", synth_hit)] {
        let measure = |sample_rate: f32| {
            let mut sum = [0.0; 3];
            for seed in 0..16 {
                let x = audible(
                    &render(sample_rate, 200.0, [2.5, 3.0, 5.0, 4.0], seed, 0.3),
                    sample_rate,
                );
                for (s, (from, to)) in sum.iter_mut().zip([(0.0, 0.005), (0.0, 0.1), (0.1, 0.3)]) {
                    *s += rms(window(&x, sample_rate, from, to)) / 16.0;
                }
            }
            sum
        };
        let reference = measure(48000.0);
        for sample_rate in RATES {
            for (part, (a, r)) in measure(sample_rate).iter().zip(&reference).enumerate() {
                let db = 20.0 * (a / r).log10();
                assert!(
                    db.abs() < 0.5,
                    "{name} at {sample_rate} Hz, part {part}: {db} dB from 48 kHz"
                );
            }
        }
    }
}

/// One sample of each drum, at `f0` cycles per sample, with every knob at
/// `knobs`.
struct Pair {
    snare: SnareLane,
    synth: SynthLane,
    snare_state: SnareState,
    synth_state: SynthSnareState,
}

impl Pair {
    fn new(sample_rate: f32, seed: u32) -> Self {
        Self {
            snare: SnareLane::new(seed),
            synth: SynthLane::new(seed),
            snare_state: SnareState::new(sample_rate),
            synth_state: SynthSnareState::new(sample_rate),
        }
    }

    fn step(&mut self, f0: f32, knobs: [f32; 4], trigger: Option<f32>) -> [f32; 2] {
        [
            self.snare
                .step(f0, snare_knobs(knobs), trigger, &self.snare_state),
            self.synth
                .step(f0, synth_knobs(knobs), trigger, &self.synth_state),
        ]
    }
}

/// With the trigger unpatched each drum drones at its pitch, as loud as accent
/// and decay allow: silent with no decay.
#[test]
fn drones_when_unpatched() {
    let drone = |knobs: [f32; 4]| {
        let mut pair = Pair::new(SR, 1);
        let out: Vec<[f32; 2]> = (0..24000)
            .map(|_| pair.step(150.0 / SR, knobs, None))
            .collect();
        [0, 1].map(|d| out.iter().map(|o| o[d]).collect::<Vec<f32>>())
    };
    for (name, x) in ["snare", "synthSnare"].iter().zip(drone(NO_SNARES)) {
        let tail = window(&x, SR, 0.25, 0.5);
        let mean = tail.iter().sum::<f32>() / tail.len() as f32;
        let ac: Vec<f32> = tail.iter().map(|v| v - mean).collect();
        assert!(rms(&ac) > 0.05, "{name} drones: {}", rms(&ac));
        let cents = pitch_cents(&x, SR, 150.0, 0.25, 0.5);
        assert!(cents.abs() < 30.0, "{name} drones at {cents} cents");
    }
    for (name, x) in ["snare", "synthSnare"]
        .iter()
        .zip(drone([2.5, 0.0, 2.5, 4.0]))
    {
        assert!(peak(window(&x, SR, 0.25, 0.5)) < 1e-6, "{name} silent");
    }
}

/// A trigger held high while it is unpatched strikes once patched again.
#[test]
fn trigger_patched_again_rises() {
    let run = |unpatch: bool| {
        let mut pair = Pair::new(SR, 1);
        (0..30000)
            .map(|n| {
                let trigger = if unpatch && (10000..12000).contains(&n) {
                    None
                } else {
                    Some(5.0)
                };
                pair.step(180.0 / SR, [2.5, 0.5, 2.5, 4.0], trigger)
            })
            .collect::<Vec<_>>()
    };
    let (held, repatched) = (run(false), run(true));
    for (d, name) in ["snare", "synthSnare"].iter().enumerate() {
        let after = |x: &[[f32; 2]]| {
            x[12000..14000]
                .iter()
                .fold(0.0f32, |m, o| m.max(o[d].abs()))
        };
        assert!(
            after(&repatched) > 10.0 * after(&held),
            "{name}: {} vs {}",
            after(&repatched),
            after(&held)
        );
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let mut pairs = [Pair::new(SR, 7), Pair::new(SR, 7)];
    for n in 0..8000 {
        let v = match n {
            500 => f32::NAN,
            501 => f32::INFINITY,
            502 => f32::NEG_INFINITY,
            _ => 2.0,
        };
        let trigger = Some(if n % 1000 == 0 { 5.0 } else { 0.0 });
        let a = pairs[0].step(0.004, [2.0; 4], trigger);
        let b = pairs[1].step(0.004, [v; 4], trigger);
        assert_eq!(a, b, "sample {n}");
    }
}

/// NaN, infinite and extreme pitches, knob jumps, unpatching and audio-rate
/// triggers at any engine rate leave both drums finite and bounded.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 44100.0, 192000.0] {
        let mut pair = Pair::new(sample_rate, 3);
        let mut peaks = [0.0f32; 2];
        for n in 0..40000 {
            let freq = match n % 7 {
                0 => f32::NAN,
                1 => f32::INFINITY,
                2 => f32::NEG_INFINITY,
                3 => 12.0,
                4 => -12.0,
                _ => (n as f32 * 0.001).sin() * 4.0,
            };
            let f0 = pitch(freq, 0.0, FmMode::Exp, 1.0 / sample_rate);
            let knob = (n % 11) as f32 * 0.5;
            let trigger = match n % 20000 {
                0..5000 => Some(if n % 2 == 0 { 5.0 } else { 0.0 }),
                5000..15000 => Some(if n % 997 == 0 { 5.0 } else { 0.0 }),
                _ => None,
            };
            let out = pair.step(f0, [knob, 5.0 - knob, knob, 5.0], trigger);
            for (p, o) in peaks.iter_mut().zip(out) {
                assert!(o.is_finite(), "{sample_rate} Hz, sample {n}");
                *p = p.max(o.abs());
            }
        }
        // The shells clip at 1; the snares' noise reaches about 4, and the
        // analog drum's band-pass swings it to about 9, without growing.
        assert!(peaks[0] < 12.0, "{sample_rate} Hz: snare peak {}", peaks[0]);
        assert!(
            peaks[1] < 12.0,
            "{sample_rate} Hz: synthSnare peak {}",
            peaks[1]
        );
    }
}
