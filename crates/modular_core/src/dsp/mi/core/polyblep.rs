//! Two-sample polyBLEP residuals for a step `t` samples before the end of the
//! current sample, as in `stmlib/dsp/polyblep.h`.

#[inline(always)]
pub fn this_blep_sample(t: f32) -> f32 {
    0.5 * t * t
}

#[inline(always)]
pub fn next_blep_sample(t: f32) -> f32 {
    let t = 1.0 - t;
    -0.5 * t * t
}

/// Integrated-BLEP residual for a slope change `t` samples before the end of
/// the current sample, on the sample after it.
#[inline(always)]
pub fn next_integrated_blep_sample(t: f32) -> f32 {
    let t1 = 0.5 * t;
    let t2 = t1 * t1;
    let t4 = t2 * t2;
    0.1875 - t1 + 1.5 * t2 - t4
}

/// Integrated-BLEP residual on the current sample.
#[inline(always)]
pub fn this_integrated_blep_sample(t: f32) -> f32 {
    next_integrated_blep_sample(1.0 - t)
}

/// When within the sample an edge `distance` cycles back fell, at `speed`
/// cycles per sample. A threshold that moves across the phase lands it outside
/// 0–1, slightly as the pitch sweeps and far when a knob jumps; bounding it to
/// −1–2 keeps Plaits' sweeps exact and the correction small, and a standstill
/// (0/0) reads as 0.
#[inline(always)]
pub fn edge_time(distance: f32, speed: f32) -> f32 {
    let t = distance / speed;
    if t.is_nan() { 0.0 } else { t.clamp(-1.0, 2.0) }
}
