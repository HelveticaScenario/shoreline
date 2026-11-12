//! Helpers from `stmlib/dsp/dsp.h`, and the rate conversion of its one-pole
//! coefficients.

/// `stmlib::SoftClip`: a cubic saturation reaching ±1 at ±3.
#[inline(always)]
pub fn soft_clip(x: f32) -> f32 {
    if x < -3.0 {
        -1.0
    } else if x > 3.0 {
        1.0
    } else {
        x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
    }
}

/// `ONE_POLE`: moves `state` the fraction `coefficient` of the way to `input`.
#[inline(always)]
pub fn one_pole(state: &mut f32, input: f32, coefficient: f32) {
    *state += coefficient * (input - *state);
}

/// `SLOPE`: [`one_pole`] with a `rise` coefficient toward a higher input and
/// `fall` toward a lower one.
#[inline(always)]
pub fn slope(state: &mut f32, input: f32, rise: f32, fall: f32) {
    let error = input - *state;
    *state += if error > 0.0 { rise } else { fall } * error;
}

/// A one-pole coefficient per sample at 48 kHz as the same time constant at
/// the rate `to_48k` describes (the rate over 48 kHz). A coefficient of 1 or
/// more follows at once at any rate, so it stays as it is.
pub fn per_sample(coefficient: f32, to_48k: f32) -> f32 {
    if coefficient >= 1.0 || to_48k == 1.0 {
        coefficient
    } else {
        (1.0 - (1.0 - f64::from(coefficient)).powf(1.0 / f64::from(to_48k))) as f32
    }
}

/// A per-sample decay factor at 48 kHz as the same time constant at the rate
/// `to_48k` describes.
pub fn decay_per_sample(decay: f32, to_48k: f32) -> f32 {
    1.0 - per_sample(1.0 - decay, to_48k)
}
