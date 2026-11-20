//! Waveforms shared by the free-running oscillators and their phase-driven
//! twins, each with a band-limited and a naive form.

/// PolyBLEP (polynomial band-limited step) residual for a ±1 step at phase 0,
/// with `dt` the absolute phase increment per sample. Add it to a naive wave
/// that steps up at phase 0; subtract it for a step down.
#[inline(always)]
pub fn poly_blep(phase: f32, dt: f32) -> f32 {
    if phase < dt {
        let t = phase / dt;
        t + t - t * t - 1.0
    } else if phase > 1.0 - dt {
        let t = (phase - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// Fraction of the cycle a pulse of `width` (0–5 V, 2.5 square) stays high.
#[inline(always)]
pub fn pulse_width(width: f32) -> f32 {
    (width / 5.0).clamp(0.01, 0.99)
}

/// Naive ±1 pulse, high from phase 0 to `pulse_width`, then low.
#[inline(always)]
pub fn naive_pulse(phase: f32, pulse_width: f32) -> f32 {
    if phase < pulse_width { 1.0 } else { -1.0 }
}

/// ±1 pulse at `phase`, high for `pulse_width` of the cycle, PolyBLEP'd at its
/// rising and falling edges when `bandlimit` is set. `dt` is the absolute phase
/// increment per sample.
#[inline(always)]
pub fn pulse(phase: f32, pulse_width: f32, dt: f32, bandlimit: bool) -> f32 {
    let naive = naive_pulse(phase, pulse_width);
    if !bandlimit {
        return naive;
    }
    let from_fall = if phase >= pulse_width {
        phase - pulse_width
    } else {
        phase - pulse_width + 1.0
    };
    naive + poly_blep(phase, dt) - poly_blep(from_fall, dt)
}

/// Symmetry (peak position) of the saw/triangle/ramp morph for `shape`
/// (0–5 V): 0 saw (peak at 1), 2.5 triangle (peak at 0.5), 5 ramp (peak at 0).
/// DPW divides by both segment lengths, so the band-limited form keeps the
/// peak off the cycle's ends; the naive form reaches them, resetting in one
/// sample.
#[inline(always)]
pub fn triangle_symmetry(shape: f32, bandlimit: bool) -> f64 {
    let s = 1.0 - shape * 0.2;
    if bandlimit {
        s.clamp(0.001, 0.999) as f64
    } else {
        s.clamp(0.0, 1.0) as f64
    }
}

/// ±1 variable-symmetry triangle at `phase`, its corners band-limited by DPW
/// (differentiated polynomial waveform) when `bandlimit` is set. `prev_phase`
/// is the previous sample's phase and `increment` the phase advanced since.
///
/// DPW differentiates the wave's antiderivative across the sample, which reads
/// the wave half a sample late. Below 1e-7 cycles per sample (0.0048 Hz at
/// 48 kHz) it reads the naive wave, where the difference vanishes.
#[inline(always)]
pub fn triangle(prev_phase: f64, phase: f64, increment: f64, s: f64, bandlimit: bool) -> f64 {
    if bandlimit && increment.abs() > 1.0e-7 {
        // In f64: at LFO rates the two integrals are near-equal, and f32
        // rounding of their difference would be amplified into per-sample hash.
        (triangle_integral(phase, s) - triangle_integral(prev_phase, s)) / increment
    } else {
        naive_triangle(phase, s)
    }
}

/// Naive ±1 variable-symmetry triangle with its peak at phase `s`; phase is in
/// `[0, 1)`, so `s = 0` and `s = 1` never divide by zero.
#[inline(always)]
pub fn naive_triangle(phase: f64, s: f64) -> f64 {
    if phase < s {
        2.0 * phase / s - 1.0
    } else {
        1.0 - 2.0 * (phase - s) / (1.0 - s)
    }
}

/// Antiderivative of [`naive_triangle`]: continuous and periodic
/// (`F(0) = F(1) = 0`), so a difference across a phase wrap stays valid.
#[inline(always)]
fn triangle_integral(phase: f64, s: f64) -> f64 {
    if phase < s {
        // f(p) = 2p/s − 1  →  F(p) = p²/s − p
        phase * phase / s - phase
    } else {
        // f(p) = 1 − 2(p − s)/(1 − s)  →  F(p) = p − (p − s)²/(1 − s) − s
        let d = phase - s;
        phase - d * d / (1.0 - s) - s
    }
}
