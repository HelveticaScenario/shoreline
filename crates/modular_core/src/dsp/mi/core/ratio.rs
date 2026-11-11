//! Plaits' quantised modulator ratios, shared by its FM-style engines.

use super::tables::LUT_FM_FREQUENCY_QUANTIZER;

/// The modulator's ratio to the carrier, in semitones, for a `ratio` knob in
/// volts: Plaits' quantised ratio table.
pub fn ratio_semitones(ratio: f32) -> f32 {
    let position = (ratio * 0.2).clamp(0.0, 1.0) * 128.0;
    let integral = position as usize;
    let fractional = position - integral as f32;
    let a = LUT_FM_FREQUENCY_QUANTIZER[integral];
    let b = LUT_FM_FREQUENCY_QUANTIZER[integral + 1];
    a + (b - a) * fractional
}

/// The frequency multiplier for `semitones`, computed in f64 and rounded so
/// every platform's libm gives the same value.
pub fn ratio_multiplier(semitones: f32) -> f32 {
    f64::from(semitones / 12.0).exp2() as f32
}
