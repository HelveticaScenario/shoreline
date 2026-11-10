//! Table reads from `stmlib/dsp/dsp.h`.

/// `stmlib::InterpolateHermite`: `table` read at `index` × `size` with
/// four-point Hermite interpolation. The read spans one entry before and two
/// after the integral index, which must lie inside `table`.
#[inline(always)]
pub fn hermite(table: &[f32], index: f32, size: f32) -> f32 {
    let index = index * size;
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
