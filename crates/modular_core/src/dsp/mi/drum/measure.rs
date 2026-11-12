//! Measurements the drum tests share.

use crate::poly::PolySignal;
use crate::types::{GlideMark, Signal};

/// The glide mark of a constant knob.
pub fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

pub fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// The part of `x` from `from` to `to` seconds.
pub fn window(x: &[f32], sample_rate: f32, from: f32, to: f32) -> &[f32] {
    &x[(from * sample_rate) as usize..(to * sample_rate) as usize]
}

/// The level in dB of `x` from `from` to `to` seconds.
pub fn level(x: &[f32], sample_rate: f32, from: f32, to: f32) -> f32 {
    20.0 * rms(window(x, sample_rate, from, to)).log10()
}

/// The strongest frequency within an octave either side of `hz`, in cents
/// from it, from `x` between `from` and `to` seconds.
pub fn pitch_cents(x: &[f32], sample_rate: f32, hz: f32, from: f32, to: f32) -> f32 {
    let x = window(x, sample_rate, from, to);
    (-1200..=1200)
        .step_by(2)
        .map(|cents| {
            let f = f64::from(hz) * 2f64.powf(f64::from(cents) / 1200.0);
            let w = std::f64::consts::TAU * f / f64::from(sample_rate);
            let (re, im) = x
                .iter()
                .enumerate()
                .fold((0.0f64, 0.0f64), |(re, im), (n, &v)| {
                    let phase = w * n as f64;
                    (
                        re + f64::from(v) * phase.cos(),
                        im + f64::from(v) * phase.sin(),
                    )
                });
            (cents, re * re + im * im)
        })
        .fold((0, 0.0), |best, c| if c.1 > best.1 { c } else { best })
        .0 as f32
}

/// How much of `x`'s energy is in its sample-to-sample change: higher for a
/// brighter or more distorted sound.
pub fn brightness(x: &[f32]) -> f32 {
    let change: f32 = x.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
    change / x.iter().map(|v| v * v).sum::<f32>()
}

/// How many times `x` crosses zero.
pub fn crossings(x: &[f32]) -> usize {
    x.windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count()
}

/// Asserts the hits `render` gives at each of `rates` have the 48 kHz hit's
/// pitch at `hz`, level and decay.
pub fn assert_rate_independent(
    name: &str,
    hz: f32,
    rates: &[f32],
    render: impl Fn(f32) -> Vec<f32>,
) {
    let measure = |sample_rate: f32| {
        let x = render(sample_rate);
        [
            pitch_cents(&x, sample_rate, hz, 0.02, 0.2),
            level(&x, sample_rate, 0.0, 0.05),
            level(&x, sample_rate, 0.05, 0.15) - level(&x, sample_rate, 0.15, 0.25),
        ]
    };
    let [cents, db, decay] = measure(48000.0);
    for &sample_rate in rates {
        let [c, d, k] = measure(sample_rate);
        assert!(
            (c - cents).abs() <= 4.0,
            "{name} at {sample_rate} Hz: {c} cents vs {cents}"
        );
        assert!(
            (d - db).abs() < 0.3,
            "{name} at {sample_rate} Hz: {d} dB vs {db}"
        );
        assert!(
            (k - decay).abs() < 0.3,
            "{name} at {sample_rate} Hz: decays {k} dB vs {decay}"
        );
    }
}
