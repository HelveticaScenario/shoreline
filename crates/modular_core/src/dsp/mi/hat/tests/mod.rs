mod knobs;

use super::channel::{Channel, HatState, Knobs};
use super::hat_drum::{RingHat, SquareHat};
use crate::dsp::mi::core::fixture::{max_error, records};
use crate::dsp::mi::drum::measure::{mark, peak, pitch_cents, rms, window};
use crate::dsp::mi::drum::pitch;
use crate::dsp::oscillators::FmMode;

/// The hats' knobs in volts: tone, decay, noisiness and accent.
fn knobs([tone, decay, noisiness, accent]: [f32; 4]) -> Knobs {
    Knobs {
        tone: (tone, mark()),
        decay: (decay, mark()),
        noisiness: (noisiness, mark()),
        accent: (accent, mark()),
    }
}

const SR: f32 = 48000.0;

/// Replays `hihat.f32` (Plaits' two hi-hats rendered a sample at a time):
/// strikes across accents and every knob's range, retriggers mid-decay and at
/// audio rate, a pitch swept by FM, pitches past the oscillators' and noise
/// clock's caps and near 1 Hz, a hiss with the trigger unpatched, strikes
/// again, and a hit ringing out through the ring hat's choke.
#[test]
fn matches_plaits() {
    let state = HatState::new(SR);
    let mut square = Channel::<SquareHat>::new(0x1234567);
    let mut ring = Channel::<RingHat>::new(0x7654321);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for r in records("hihat", 7 + 2) {
        let trigger = (r[5] == 0.0).then_some(if r[6] != 0.0 { 5.0 } else { 0.0 });
        let knobs = || knobs([r[1], r[2], r[3], r[4]]);
        actual.push(square.step(r[0], knobs(), trigger, &state));
        actual.push(ring.step(r[0], knobs(), trigger, &state));
        expected.extend([r[7], r[8]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(
        error < 1e-5,
        "max error {error} at sample {} ({})",
        at / 2,
        ["hiHat", "ringHat"][at % 2]
    );
}

/// `seconds` of a hat `step` renders at `sample_rate`, struck once after its
/// filters settle from a fresh start.
fn hit(sample_rate: f32, seconds: f32, mut step: impl FnMut(Option<f32>) -> f32) -> Vec<f32> {
    let settle = (0.2 * sample_rate) as usize;
    (0..settle + (seconds * sample_rate) as usize)
        .map(|n| step(Some(if n == settle { 5.0 } else { 0.0 })))
        .skip(settle)
        .collect()
}

/// `seconds` of `$mi.hiHat` at `hz` with noise from `seed`, struck once.
fn square_hit(sample_rate: f32, hz: f32, volts: [f32; 4], seed: u32, seconds: f32) -> Vec<f32> {
    let state = HatState::new(sample_rate);
    let mut channel = Channel::<SquareHat>::new(seed);
    hit(sample_rate, seconds, |trigger| {
        channel.step(hz / sample_rate, knobs(volts), trigger, &state)
    })
}

/// `seconds` of `$mi.ringHat` at `hz` with noise from `seed`, struck once.
fn ring_hit(sample_rate: f32, hz: f32, volts: [f32; 4], seed: u32, seconds: f32) -> Vec<f32> {
    let state = HatState::new(sample_rate);
    let mut channel = Channel::<RingHat>::new(seed);
    hit(sample_rate, seconds, |trigger| {
        channel.step(hz / sample_rate, knobs(volts), trigger, &state)
    })
}

type Render = fn(f32, f32, [f32; 4], u32, f32) -> Vec<f32>;
const HATS: [(&str, Render); 2] = [("hiHat", square_hit), ("ringHat", ring_hit)];

const RATES: [f32; 3] = [44100.0, 96000.0, 192000.0];

/// No noise, the other knobs at their defaults.
const METAL: [f32; 4] = [2.5, 2.5, 0.0, 4.0];

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

/// The centre of `x`'s energy from 100 Hz to 12 kHz, in Hz, from its
/// Hann-windowed spectrum every 100 Hz.
fn centroid(x: &[f32], sample_rate: f32) -> f32 {
    let len = x.len() as f64;
    let power = |hz: f64| {
        let w = std::f64::consts::TAU * hz / f64::from(sample_rate);
        let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (n, &v)| {
            let window = 0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / len).cos();
            let v = f64::from(v) * window;
            let phase = w * n as f64;
            (re + v * phase.cos(), im + v * phase.sin())
        });
        re * re + im * im
    };
    let (mut moment, mut all) = (0.0, 0.0);
    for k in 1..=120 {
        let hz = 100.0 * f64::from(k);
        let p = power(hz);
        moment += hz * p;
        all += p;
    }
    (moment / all) as f32
}

/// With no noise, a strike has the same level and decay in the audible band,
/// and the same spectral centre, at any rate.
#[test]
fn metal_sounds_the_same_at_any_rate() {
    for (hz, volts) in [
        (200.0, METAL),
        (400.0, [4.0, 3.5, 0.0, 5.0]),
        (120.0, [1.0, 1.5, 0.0, 2.0]),
    ] {
        for (name, render) in HATS {
            let measure = |sample_rate: f32| {
                let raw = render(sample_rate, hz, volts, 1, 0.3);
                let x = audible(&raw, sample_rate);
                let level = |from, to| 20.0 * rms(window(&x, sample_rate, from, to)).log10();
                [
                    level(0.0, 0.05),
                    level(0.0, 0.05) - level(0.05, 0.15),
                    1200.0 * centroid(window(&raw, sample_rate, 0.0, 0.05), sample_rate).log2(),
                ]
            };
            let reference = measure(SR);
            for sample_rate in RATES {
                let actual = measure(sample_rate);
                // Within 0.5 dB, and the centre within 50 cents.
                for (part, (a, r, tolerance)) in actual
                    .iter()
                    .zip(&reference)
                    .zip([0.5, 0.5, 50.0])
                    .map(|((a, r), t)| (a, r, t))
                    .enumerate()
                {
                    assert!(
                        (a - r).abs() < tolerance,
                        "{name} at {hz} Hz, {sample_rate} Hz, part {part}: {a} vs {r}"
                    );
                }
            }
        }
    }
}

/// The clocked noise of a strike is as loud, and fades as fast, at any rate,
/// over many noise draws.
#[test]
fn noise_sounds_the_same_at_any_rate() {
    for (name, render) in HATS {
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
        let reference = measure(SR);
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

/// The ring hat's partials sit at the same frequencies at any rate: its
/// hiss, held steady with the trigger unpatched, peaks at the same pitch.
#[test]
fn ring_partials_hold_their_pitch_at_any_rate() {
    let hiss = |sample_rate: f32| {
        let state = HatState::new(sample_rate);
        let mut channel = Channel::<RingHat>::new(1);
        let volts = [0.0, 5.0, 0.0, 5.0];
        (0..(0.5 * sample_rate) as usize)
            .map(|_| channel.step(1000.0 / sample_rate, knobs(volts), None, &state))
            .collect::<Vec<f32>>()
    };
    // The lowest pair rings at 200 Hz ± the saw's partials, scaled by
    // 2000 / 2480 at a 1 kHz pitch.
    let partial = 7530.0 * 2000.0 / 2480.0 - 200.0 * 2000.0 / 2480.0;
    let reference = pitch_cents(&hiss(SR), SR, partial, 0.25, 0.5);
    for sample_rate in RATES {
        let cents = pitch_cents(&hiss(sample_rate), sample_rate, partial, 0.25, 0.5);
        assert!(
            (cents - reference).abs() <= 4.0,
            "{sample_rate} Hz: {cents} cents vs {reference}"
        );
    }
}

/// With the trigger unpatched each hat hisses, as loud as accent and decay
/// allow: silent with no decay.
#[test]
fn hisses_when_unpatched() {
    for (name, hiss) in [
        ("hiHat", hiss::<SquareHat> as fn([f32; 4]) -> Vec<f32>),
        ("ringHat", hiss::<RingHat>),
    ] {
        let loud = hiss(METAL);
        assert!(rms(window(&loud, SR, 0.25, 0.5)) > 0.05, "{name} hisses");
        let quiet = hiss([2.5, 2.5, 0.0, 1.0]);
        assert!(
            rms(window(&quiet, SR, 0.25, 0.5)) < 0.5 * rms(window(&loud, SR, 0.25, 0.5)),
            "{name} quieter with less accent"
        );
        let silent = hiss([2.5, 0.0, 2.5, 4.0]);
        assert!(peak(window(&silent, SR, 0.25, 0.5)) < 1e-6, "{name} silent");
    }
}

/// Half a second of a hat with its trigger unpatched, at 300 Hz.
fn hiss<M: super::hat_drum::Model>(volts: [f32; 4]) -> Vec<f32> {
    let state = HatState::new(SR);
    let mut channel = Channel::<M>::new(1);
    (0..24000)
        .map(|_| channel.step(300.0 / SR, knobs(volts), None, &state))
        .collect()
}

/// Both hats stepped alike.
struct Pair {
    square: Channel<SquareHat>,
    ring: Channel<RingHat>,
    state: HatState,
}

impl Pair {
    fn new(sample_rate: f32, seed: u32) -> Self {
        Self {
            square: Channel::new(seed),
            ring: Channel::new(seed),
            state: HatState::new(sample_rate),
        }
    }

    fn step(&mut self, f0: f32, volts: [f32; 4], trigger: Option<f32>) -> [f32; 2] {
        [
            self.square.step(f0, knobs(volts), trigger, &self.state),
            self.ring.step(f0, knobs(volts), trigger, &self.state),
        ]
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
                pair.step(300.0 / SR, [2.5, 0.5, 2.5, 4.0], trigger)
            })
            .collect::<Vec<_>>()
    };
    let (held, repatched) = (run(false), run(true));
    for (d, name) in ["hiHat", "ringHat"].iter().enumerate() {
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
/// triggers at any engine rate leave both hats finite and bounded.
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
        assert!(peaks[0] < 12.0, "{sample_rate} Hz: hiHat peak {}", peaks[0]);
        assert!(
            peaks[1] < 12.0,
            "{sample_rate} Hz: ringHat peak {}",
            peaks[1]
        );
    }
}
