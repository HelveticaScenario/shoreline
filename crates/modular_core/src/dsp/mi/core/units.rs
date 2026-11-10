//! `stmlib::SemitonesToRatio`: a frequency ratio from two table reads, with no
//! transcendental call, so it gives the same value on every platform.

use super::tables::{LUT_PITCH_RATIO_HIGH, LUT_PITCH_RATIO_LOW};

/// The frequency ratio of `semitones`, for ±128 semitones; values past that
/// read the table's ends.
#[inline(always)]
pub fn semitones_to_ratio(semitones: f32) -> f32 {
    let pitch = (semitones + 128.0).clamp(0.0, 255.999);
    let integral = pitch as i32;
    let fractional = pitch - integral as f32;
    LUT_PITCH_RATIO_HIGH[integral as usize]
        * LUT_PITCH_RATIO_LOW[(fractional * 256.0) as i32 as usize]
}
