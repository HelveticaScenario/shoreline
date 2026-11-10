use super::tables::LUT_SINE;

const SINE_LUT_BITS: u32 = 9;
const SINE_LUT_SIZE: f32 = (1 << SINE_LUT_BITS) as f32;

/// Plaits' `Sine`: one cycle per unit of `phase`, read from a 512-point table
/// with linear interpolation. Wraps any finite phase.
#[inline(always)]
pub fn sine(phase: f32) -> f32 {
    let index = (phase - phase.floor()) * SINE_LUT_SIZE;
    let integral = index as usize;
    let fractional = index - integral as f32;
    let a = LUT_SINE[integral];
    let b = LUT_SINE[integral + 1];
    a + (b - a) * fractional
}

/// Plaits' `SinePM`: the table read at a 32-bit `phase` offset by `pm` cycles,
/// for `pm` within ±32. The offset is truncated to 1/2²⁶ of a cycle, as in Plaits.
#[inline(always)]
pub fn sine_pm(phase: u32, pm: f32) -> f32 {
    const MAX_INDEX: u32 = 32;
    const SCALE: f32 = 4294967296.0 / (MAX_INDEX * 2) as f32;
    let offset = ((pm + MAX_INDEX as f32) * SCALE) as u32;
    let phase = phase.wrapping_add(offset.wrapping_mul(MAX_INDEX * 2));
    let integral = (phase >> (32 - SINE_LUT_BITS)) as usize;
    let fractional = (phase << SINE_LUT_BITS) as f32 / 4294967296.0;
    let a = LUT_SINE[integral];
    let b = LUT_SINE[integral + 1];
    a + (b - a) * fractional
}

/// `cycles` per step as a 32-bit phase increment. A negative frequency wraps
/// to the increment that runs the phase backward.
#[inline(always)]
pub fn phase_increment(cycles: f32) -> u32 {
    (4294967296.0 * cycles) as i64 as u32
}
