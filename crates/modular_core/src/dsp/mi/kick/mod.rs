//! Plaits' two bass drums, which its bass drum engine plays side by side:
//! `$mi.kick`, the 808-style analog model through Plaits' overdrive, and
//! `$mi.synthKick`, the synthetic one.

mod analog;
mod analog_drum;
mod synthetic;
mod synthetic_drum;

pub use analog::MiKick;
pub use synthetic::MiSynthKick;

use crate::dsp::{
    mi::core::phase::limit_frequency,
    oscillators::{FmMode, apply_fm},
    utils::SchmittTrigger,
};
use crate::types::{GlideMark, SwapGlide};

/// Plaits' out and aux gains for this engine, at ±5 V full scale. Plaits'
/// output clips at full scale.
const OUTPUT_GAIN: f32 = -0.8 * 5.0;

/// The pitch in cycles per sample for `freq` bent by `fm`. A drum rings at
/// the magnitude of a negative frequency, so through-zero FM acts as linear
/// FM; NaN and anything below 1 Hz ring at 1 Hz.
#[inline(always)]
fn pitch(freq: f32, fm: f32, fm_mode: FmMode, inv_sample_rate: f32) -> f32 {
    let f0 = apply_fm(freq, fm, fm_mode) * inv_sample_rate;
    limit_frequency(f0.abs(), 0.5).max(inv_sample_rate)
}

/// Whether `trigger` rises this sample; `None` is an unpatched trigger,
/// after which a trigger patched again starts from low.
#[inline(always)]
fn rises(schmitt: &mut SchmittTrigger, trigger: Option<f32>) -> bool {
    match trigger {
        Some(volts) => schmitt.process(volts),
        None => {
            *schmitt = SchmittTrigger::default();
            false
        }
    }
}

/// A 0–5 V knob as Plaits' 0–1, gliding across patch updates and holding its
/// last finite value through NaN and infinite input.
struct Knob {
    glide: SwapGlide,
    held: f32,
}

impl Knob {
    fn new(default: f32) -> Self {
        Self {
            glide: SwapGlide::default(),
            held: default,
        }
    }

    #[inline(always)]
    fn read(&mut self, (volts, mark): (f32, GlideMark), sample_rate: f32) -> f32 {
        let value = self.glide.follow(volts, mark, sample_rate);
        if value.is_finite() {
            self.held = value;
        }
        (self.held * 0.2).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests;
