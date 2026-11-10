//! Table reads from `stmlib/dsp/dsp.h`.

/// `stmlib::InterpolateHermite`: `table` read at `index` × `size` with
/// four-point Hermite interpolation, which spans one entry before and two
/// after the integral index. A position too near either end of `table` (or
/// NaN) reads at the nearest one the span fits.
#[inline(always)]
pub fn hermite(table: &[f32], index: f32, size: f32) -> f32 {
    let index = index * size;
    let last = (table.len() - 3) as f32;
    let index = if index >= 1.0 { index.min(last) } else { 1.0 };
    let integral = index as i32;
    let f = index - integral as f32;
    let i = integral as usize;
    let xm1 = table[i - 1];
    let x0 = table[i];
    let x1 = table[i + 1];
    let x2 = table[i + 2];
    let c = (x1 - xm1) * 0.5;
    let v = x0 - x1;
    let w = c + v;
    let a = w + v + (x2 - x0) * 0.5;
    let b_neg = w + a;
    (((a * f) - b_neg) * f + c) * f + x0
}

/// `stmlib::Interpolate`: `table` read at `index` × `size` with linear
/// interpolation. A position past either end reads the end pair.
#[inline(always)]
pub fn linear(table: &[f32], index: f32, size: f32) -> f32 {
    let index = index * size;
    let integral = (index as i32).clamp(0, table.len() as i32 - 2) as usize;
    let fractional = index - integral as f32;
    let a = table[integral];
    let b = table[integral + 1];
    a + (b - a) * fractional
}
