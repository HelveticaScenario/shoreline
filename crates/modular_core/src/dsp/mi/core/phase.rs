/// Brings a phase that fell below 0 by less than a cycle back into `[0, 1)`.
#[inline(always)]
pub fn wrap_below(phase: f32) -> f32 {
    if phase < 0.0 { phase + 1.0 } else { phase }
}

/// The largest `f32` below 1.
pub const BELOW_ONE: f32 = 1.0 - f32::EPSILON / 2.0;

/// `phase` brought into `[0, 1)` by whole cycles. A hair below 0 rounds up to
/// 1, and lands just below it instead, at the top of the cycle it is in.
#[inline(always)]
pub fn wrap(phase: f32) -> f32 {
    (phase - phase.floor()).min(BELOW_ONE)
}

/// The fractional part of a cycle count; 0 from 2^24 cycles on, where an `f32`
/// holds no fraction, and for an infinite count.
#[inline(always)]
pub fn fraction(cycles: f32) -> f32 {
    cycles.min(16_777_216.0).fract()
}

/// Plaits' `kMaxFrequency`, a quarter of its 48 kHz rate, in cycles per sample
/// at `sample_rate`.
pub fn max_frequency(sample_rate: f32) -> f32 {
    12000.0 / sample_rate
}

/// `f` (cycles per sample) limited to ±`max`, and 0 when NaN.
#[inline(always)]
pub fn limit_frequency(f: f32, max: f32) -> f32 {
    if f.is_nan() { 0.0 } else { f.clamp(-max, max) }
}
