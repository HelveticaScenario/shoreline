//! `plaits::AdditiveEngine`'s spectrum: harmonic amplitudes shaped around a
//! centroid, and `plaits::HarmonicOscillator`'s Chebyshev recurrence that
//! sums them.

use crate::dsp::mi::core::sine::sine;

/// Harmonics an oscillator bank renders.
pub const BANK: usize = 12;
/// Harmonics in the integer series.
pub const INTEGER: usize = 24;
/// Amplitude slots: the integer series' harmonics, then the organ bank's.
pub const SLOTS: usize = INTEGER + BANK;
/// Where the organ bank's amplitudes start.
pub const ORGAN: usize = INTEGER;

/// The integer series' slots: harmonics 1 to 24.
pub const INTEGER_SLOTS: [usize; INTEGER] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
];
/// The organ series' slots in its bank: harmonics 1, 2, 3, 4, 6, 8, 10 and
/// 12, a drawbar organ's footages.
pub const ORGAN_SLOTS: [usize; 8] = [0, 1, 2, 3, 5, 7, 9, 11];

/// Plaits' `UpdateAmplitudes`: one step of `amplitudes`' slots toward the
/// spectrum around `centroid` (0–1 across the slots), then renormalised.
/// Plaits takes this step once per 12 samples at 48 kHz; how fast the
/// spectrum moves depends on that step, so it keeps that timebase.
pub fn update_amplitudes(
    centroid: f32,
    slope: f32,
    bumps: f32,
    amplitudes: &mut [f32],
    slots: &[usize],
) {
    let n = slots.len() as f32 - 1.0;
    let margin = (1.0 / slope - 1.0) / (1.0 + bumps);
    let center = centroid * (n + margin) - 0.5 * margin;

    let mut sum = 0.001;
    for (i, &slot) in slots.iter().enumerate() {
        let order = (i as f32 - center).abs() * slope;
        let mut gain = 1.0 - order;
        gain += gain.abs();
        gain *= gain;
        gain *= 1.0 + sine(0.25 + order * bumps);
        gain *= gain;
        gain *= gain;
        amplitudes[slot] += 0.001 * (gain - amplitudes[slot]);
        sum += amplitudes[slot];
    }
    let sum = 1.0 / sum;
    for &slot in slots {
        amplitudes[slot] *= sum;
    }
}

/// One sample of a bank of harmonics `FIRST..FIRST + 12` at `phase`, where
/// `two_x` is twice the fundamental's sine.
#[inline(always)]
pub fn bank<const FIRST: usize>(phase: f32, two_x: f32, amplitudes: &[f32]) -> f32 {
    let (mut previous, mut current) = if FIRST == 1 {
        (1.0, two_x * 0.5)
    } else {
        let k = FIRST as f32;
        (sine(phase * (k - 1.0) + 0.25), sine(phase * k))
    };
    let mut sum = 0.0;
    for &amplitude in &amplitudes[..BANK] {
        sum += amplitude * current;
        let next = two_x * current - previous;
        previous = current;
        current = next;
    }
    sum
}
