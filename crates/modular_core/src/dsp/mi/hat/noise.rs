//! Plaits' two metallic noise sources for its hi-hats: `SquareNoise`, the
//! 808's six square oscillators, and `RingModNoise`, three ring-modulated
//! square and saw pairs.

use crate::dsp::mi::core::oscillator::BlepOscillator;

use super::hat_drum::HatRates;

/// A metallic noise source, set from the hat's pitch.
pub trait MetallicNoise: Default {
    /// Tunes the source to `f0`, twice the hat's pitch, in cycles per sample.
    fn set_pitch(&mut self, f0: f32, rates: &HatRates);

    fn next(&mut self) -> f32;
}

/// The six oscillators' frequencies over `f0`, for a nominal 414 Hz.
const SQUARE_RATIOS: [f32; 6] = [1.0, 1.304, 1.466, 1.787, 1.932, 2.536];

/// Six naive square waves summed, as the 808's six Schmitt trigger
/// oscillators.
#[derive(Default)]
pub struct SquareNoise {
    phase: [u32; 6],
    increment: [u32; 6],
}

impl MetallicNoise for SquareNoise {
    fn set_pitch(&mut self, f0: f32, rates: &HatRates) {
        for (increment, ratio) in self.increment.iter_mut().zip(SQUARE_RATIOS) {
            let f = (f0 * ratio).min(rates.max_square);
            *increment = (f * 4294967296.0) as u32;
        }
    }

    #[inline(always)]
    fn next(&mut self) -> f32 {
        let mut noise = 0;
        for (phase, increment) in self.phase.iter_mut().zip(self.increment) {
            *phase = phase.wrapping_add(increment);
            noise += *phase >> 31;
        }
        0.33 * noise as f32 - 1.0
    }
}

/// Each pair's square and saw frequencies in Hz, which the pitch scales by
/// `f0 / (f0 + 0.01)` at 48 kHz.
const RING_HZ: [f32; 6] = [200.0, 7530.0, 510.0, 8075.0, 730.0, 10500.0];

/// Three products of a square and a saw, summed.
#[derive(Default)]
pub struct RingModNoise {
    oscillators: [BlepOscillator; 6],
    frequency: [f32; 6],
}

impl MetallicNoise for RingModNoise {
    fn set_pitch(&mut self, f0: f32, rates: &HatRates) {
        let f0 = f0 * rates.to_48k;
        let ratio = f0 / (0.01 + f0);
        for (frequency, per_hz) in self.frequency.iter_mut().zip(rates.ring_frequencies) {
            *frequency = per_hz * ratio;
        }
    }

    #[inline(always)]
    fn next(&mut self) -> f32 {
        let mut out = 0.0;
        for (pair, f) in self
            .oscillators
            .chunks_exact_mut(2)
            .zip(self.frequency.chunks_exact(2))
        {
            let square = pair[0].square(f[0]);
            out += square * pair[1].saw(f[1]);
        }
        out
    }
}

/// `RING_HZ` in cycles per sample at `sample_rate`.
pub fn ring_frequencies(sample_rate: f32) -> [f32; 6] {
    RING_HZ.map(|hz| hz / sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ring hat's partials climb with the pitch and level off above
    /// about 2 kHz.
    #[test]
    fn ring_partials_level_off() {
        let rates = HatRates::new(48000.0);
        let top = |hz: f32| {
            let mut noise = RingModNoise::default();
            noise.set_pitch(2.0 * hz / 48000.0, &rates);
            noise.frequency[5] * 48000.0
        };
        assert!(top(2000.0) > 1.9 * top(200.0));
        assert!(top(8000.0) < 1.12 * top(2000.0));
    }
}
