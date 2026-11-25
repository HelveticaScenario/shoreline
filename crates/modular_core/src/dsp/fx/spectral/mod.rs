//! Reverbs that work on the spectrum of the input instead of on delay lines.

pub mod echo;
mod stft;
#[cfg(test)]
mod test_support;
pub mod wash;

pub use echo::SpectralEcho;
pub use wash::SpectralWash;

const MIN_T60: f32 = 0.2;
const MAX_T60: f32 = 60.0;
/// `ln(1000)`: the −60 dB factor in a decay gain.
const LN_1000: f32 = 6.907_755;

/// Positive damping shortens the decay above this frequency.
const DAMP_HIGH_FROM_HZ: f32 = 100.0;
/// Negative damping shortens the decay below this frequency.
const DAMP_LOW_FROM_HZ: f32 = 10_000.0;
/// Octaves of decay time lost per octave of frequency at full damping.
const DAMP_SLOPE: f32 = 0.6;

/// Largest input level compensation exponent: see [`input_gain`].
const BUILD_UP_SHARE: f32 = 0.5;

/// Decay time in seconds for a `decay` voltage.
fn decay_seconds(decay: f32) -> f32 {
    MIN_T60 * (MAX_T60 / MIN_T60).powf(decay.clamp(0.0, 5.0) / 5.0)
}

/// Decay time at `freq` for a `damping` voltage: positive damping shortens
/// the highs, negative the lows.
fn damped_seconds(seconds: f32, freq: f32, damping: f32) -> f32 {
    let amount = damping.clamp(-5.0, 5.0) / 5.0;
    let octaves = if amount >= 0.0 {
        (freq / DAMP_HIGH_FROM_HZ).log2()
    } else {
        (DAMP_LOW_FROM_HZ / freq).log2()
    };
    seconds * (-amount.abs() * DAMP_SLOPE * octaves.max(0.0)).exp2()
}

/// Gain left after `seconds_passed` of a tail that falls 60 dB in `t60`.
fn decay_gain(seconds_passed: f32, t60: f32) -> f32 {
    (-LN_1000 * seconds_passed / t60).exp()
}

/// Power gain on the input of a loop that keeps `loop_power` of its power
/// each pass. A steady input builds up by `1 / (1 - loop_power)`; this takes
/// back half of that in decibels, so long tails neither swamp a held note
/// nor bury a short one.
fn input_gain(loop_power: f32) -> f32 {
    (1.0 - loop_power).max(0.0).powf(BUILD_UP_SHARE)
}

/// Frequency ratio per second of tail for a `shimmer` voltage: an octave at
/// either end.
fn shimmer_ratio_per_second(shimmer: f32) -> f32 {
    (shimmer.clamp(-5.0, 5.0) / 5.0).exp2()
}
