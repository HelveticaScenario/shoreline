//! Four lanes at a time. A group of four lanes runs as one `f32x4` per
//! signal; the few parts with no vector form run per element, on the active elements
//! only. Elements past the lane count are dead: they compute on silence and
//! never reach an output.

pub(super) use wide::f32x4;

use std::ops::{Add, Div, Mul, Neg, Sub};

use crate::dsp::utils::{
    halfband::{BRANCH_A, BRANCH_B},
    sanitize,
};

/// Lanes per group.
pub(super) const SIMD_WIDTH: usize = 4;

/// Every element set to `value`, broadcast in a register so that no use lowers to
/// a `memset_pattern16` call on Apple targets.
#[inline(always)]
fn broadcast(value: f32) -> f32x4 {
    // SAFETY: both types are four `f32` elements, and every bit pattern is a valid
    // value of each.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        std::mem::transmute(std::arch::aarch64::vdupq_n_f32(value))
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        std::mem::transmute(std::arch::x86_64::_mm_set1_ps(value))
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    f32x4::splat(value)
}

/// The value type the filter computes with, so one kernel serves both widths:
/// `f32x4` runs a whole group and `f32` runs element 0 alone, a single-lane
/// module's one lane, without paying for three dead elements. State is stored
/// as `f32x4` either way.
pub(super) trait Simd:
    Copy
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
{
    /// Per-element flags: element masks for `f32x4`, a `bool` for `f32`.
    type Mask: Copy;
    /// Elements computed: 4 for `f32x4`, element 0 alone for `f32`.
    const WIDTH: usize;

    fn splat(value: f32) -> Self;
    fn load(stored: &f32x4) -> Self;
    fn store(self, stored: &mut f32x4);
    /// Element values, for work with no vector form; `f32` fills element 0.
    fn to_elements(self) -> [f32; SIMD_WIDTH];
    fn from_elements(elements: [f32; SIMD_WIDTH]) -> Self;
    /// Non-finite elements become 0, element for element as `sanitize` does.
    fn sanitize(self) -> Self;
    fn abs(self) -> Self;
    fn min(self, rhs: Self) -> Self;
    fn max(self, rhs: Self) -> Self;
    fn sqrt(self) -> Self;
    fn lt(self, rhs: Self) -> Self::Mask;
    fn ge(self, rhs: Self) -> Self::Mask;
    fn select(mask: Self::Mask, if_true: Self, if_false: Self) -> Self;
    /// Bit `l` set where element `l`'s flag is.
    fn bits(mask: Self::Mask) -> u32;
    /// Linear interpolation in `table` at fractional positions `pos`. Positions
    /// are held inside the table and NaN ones read its first entry.
    fn lerp(table: &[f32], pos: Self) -> Self;
}

impl Simd for f32x4 {
    type Mask = f32x4;
    const WIDTH: usize = SIMD_WIDTH;

    #[inline(always)]
    fn splat(value: f32) -> Self {
        broadcast(value)
    }
    #[inline(always)]
    fn load(stored: &f32x4) -> Self {
        *stored
    }
    #[inline(always)]
    fn store(self, stored: &mut f32x4) {
        *stored = self;
    }
    #[inline(always)]
    fn to_elements(self) -> [f32; SIMD_WIDTH] {
        self.to_array()
    }
    #[inline(always)]
    fn from_elements(elements: [f32; SIMD_WIDTH]) -> Self {
        f32x4::new(elements)
    }
    // `x − x` is zero exactly when `x` is finite.
    #[allow(clippy::eq_op)]
    #[inline(always)]
    fn sanitize(self) -> Self {
        (self - self)
            .simd_eq(f32x4::default())
            .select(self, f32x4::default())
    }
    #[inline(always)]
    fn abs(self) -> Self {
        f32x4::abs(self)
    }
    #[inline(always)]
    fn min(self, rhs: Self) -> Self {
        f32x4::min(self, rhs)
    }
    #[inline(always)]
    fn max(self, rhs: Self) -> Self {
        f32x4::max(self, rhs)
    }
    #[inline(always)]
    fn sqrt(self) -> Self {
        f32x4::sqrt(self)
    }
    #[inline(always)]
    fn lt(self, rhs: Self) -> Self::Mask {
        self.simd_lt(rhs)
    }
    #[inline(always)]
    fn ge(self, rhs: Self) -> Self::Mask {
        self.simd_ge(rhs)
    }
    #[inline(always)]
    fn select(mask: Self::Mask, if_true: Self, if_false: Self) -> Self {
        mask.select(if_true, if_false)
    }
    #[inline(always)]
    fn bits(mask: Self::Mask) -> u32 {
        mask.to_bitmask()
    }
    #[inline]
    fn lerp(table: &[f32], pos: Self) -> Self {
        let last = broadcast((table.len() - 1) as f32 * (1.0 - f32::EPSILON));
        // `max` passes over a NaN, so a NaN position reads the first entry.
        let pos = pos.max(f32x4::default()).min(last);
        let k = pos.trunc_int();
        let frac = pos - f32x4::from_i32x4(k);
        let k = k.to_array().map(|k| k as usize);
        let a = f32x4::new(k.map(|k| table[k]));
        let b = f32x4::new(k.map(|k| table[k + 1]));
        a + (b - a) * frac
    }
}

impl Simd for f32 {
    type Mask = bool;
    const WIDTH: usize = 1;

    #[inline(always)]
    fn splat(value: f32) -> Self {
        value
    }
    #[inline(always)]
    fn load(stored: &f32x4) -> Self {
        stored.as_array()[0]
    }
    #[inline(always)]
    fn store(self, stored: &mut f32x4) {
        stored.as_mut_array()[0] = self;
    }
    #[inline(always)]
    fn to_elements(self) -> [f32; SIMD_WIDTH] {
        [self, 0.0, 0.0, 0.0]
    }
    #[inline(always)]
    fn from_elements(elements: [f32; SIMD_WIDTH]) -> Self {
        elements[0]
    }
    #[inline(always)]
    fn sanitize(self) -> Self {
        sanitize(self)
    }
    #[inline(always)]
    fn abs(self) -> Self {
        f32::abs(self)
    }
    #[inline(always)]
    fn min(self, rhs: Self) -> Self {
        f32::min(self, rhs)
    }
    #[inline(always)]
    fn max(self, rhs: Self) -> Self {
        f32::max(self, rhs)
    }
    #[inline(always)]
    fn sqrt(self) -> Self {
        f32::sqrt(self)
    }
    #[inline(always)]
    fn lt(self, rhs: Self) -> Self::Mask {
        self < rhs
    }
    #[inline(always)]
    fn ge(self, rhs: Self) -> Self::Mask {
        self >= rhs
    }
    #[inline(always)]
    fn select(mask: Self::Mask, if_true: Self, if_false: Self) -> Self {
        if mask { if_true } else { if_false }
    }
    #[inline(always)]
    fn bits(mask: Self::Mask) -> u32 {
        mask as u32
    }
    #[inline]
    fn lerp(table: &[f32], pos: Self) -> Self {
        // `max` passes over a NaN, so a NaN position reads the first entry.
        let pos = pos
            .max(0.0)
            .min((table.len() - 1) as f32 * (1.0 - f32::EPSILON));
        let k = pos as usize;
        let (a, b) = (table[k], table[k + 1]);
        a + (b - a) * (pos - k as f32)
    }
}

/// Clears element `element` of `v`.
#[inline]
pub(super) fn clear_element(v: &mut f32x4, element: usize) {
    v.as_mut_array()[element] = 0.0;
}

/// Sets element `element` of `v` to `value`.
#[inline]
pub(super) fn set_element(v: &mut f32x4, element: usize, value: f32) {
    v.as_mut_array()[element] = value;
}

/// A first-order allpass section, `y[n] = a·(x[n] − y[n−1]) + x[n−1]`, per
/// element; its state is sanitized every sample as the scalar section's is.
#[derive(Clone, Copy, Default)]
struct Allpass {
    x_prev: f32x4,
    y_prev: f32x4,
}

impl Allpass {
    #[inline]
    fn process<L: Simd>(&mut self, coeff: f32, x: L) -> L {
        let y = (L::splat(coeff) * (x - L::load(&self.y_prev)) + L::load(&self.x_prev)).sanitize();
        x.sanitize().store(&mut self.x_prev);
        y.store(&mut self.y_prev);
        y
    }

    fn clear_element(&mut self, element: usize) {
        clear_element(&mut self.x_prev, element);
        clear_element(&mut self.y_prev, element);
    }
}

/// The shared polyphase halfband, as a 2× upsampler per element.
#[derive(Clone, Copy, Default)]
pub(super) struct HalfbandUp {
    stages: [Allpass; 4],
}

impl HalfbandUp {
    /// Returns `(even, odd)` at the upper rate.
    #[inline]
    pub(super) fn process<L: Simd>(&mut self, x: L) -> (L, L) {
        let [a1, a2, b1, b2] = &mut self.stages;
        let even = a2.process(BRANCH_A[1], a1.process(BRANCH_A[0], x));
        let odd = b2.process(BRANCH_B[1], b1.process(BRANCH_B[0], x));
        (even, odd)
    }

    pub(super) fn clear_element(&mut self, element: usize) {
        self.stages
            .iter_mut()
            .for_each(|s| s.clear_element(element));
    }
}

/// The shared polyphase halfband, as a 2× downsampler per element.
#[derive(Clone, Copy, Default)]
pub(super) struct HalfbandDown {
    stages: [Allpass; 4],
}

impl HalfbandDown {
    /// One lower-rate sample from an even/odd pair, even first in time.
    #[inline]
    pub(super) fn process<L: Simd>(&mut self, even: L, odd: L) -> L {
        let [a1, a2, b1, b2] = &mut self.stages;
        let a = a2.process(BRANCH_A[1], a1.process(BRANCH_A[0], odd));
        let b = b2.process(BRANCH_B[1], b1.process(BRANCH_B[0], even));
        (a + b) * L::splat(0.5)
    }

    pub(super) fn clear_element(&mut self, element: usize) {
        self.stages
            .iter_mut()
            .for_each(|s| s.clear_element(element));
    }

    /// Swaps element `l` of `downs[k]` with element `k` of `downs[l]`, moving four
    /// signals of one element into one downsampler's four elements or back.
    pub(super) fn transpose(downs: &mut [Self; 4]) {
        for stage in 0..4 {
            transpose(downs.each_mut().map(|d| &mut d.stages[stage].x_prev));
            transpose(downs.each_mut().map(|d| &mut d.stages[stage].y_prev));
        }
    }
}

/// Transposes the 4×4 matrix whose rows are `rows`.
fn transpose(rows: [&mut f32x4; 4]) {
    let m = rows.each_ref().map(|r| r.to_array());
    for (k, row) in rows.into_iter().enumerate() {
        *row = f32x4::new(std::array::from_fn(|l| m[l][k]));
    }
}

/// One-pole lowpass per element, with one coefficient for the group.
#[derive(Clone, Copy, Default)]
pub(super) struct OnePole {
    coeff: f32,
    state: f32x4,
}

impl OnePole {
    pub(super) fn set_coeff(&mut self, coeff: f32) {
        self.coeff = coeff;
    }

    #[inline]
    pub(super) fn process<L: Simd>(&mut self, x: L) -> L {
        let y = (x * L::splat(self.coeff) + L::load(&self.state) * L::splat(1.0 - self.coeff))
            .sanitize();
        y.store(&mut self.state);
        y
    }

    pub(super) fn clear_element(&mut self, element: usize) {
        clear_element(&mut self.state, element);
    }
}

/// The inputs a control law was last derived from, per element: each element
/// recomputes when its input's bits change, as `Cached` does for one lane.
#[derive(Clone, Copy, Default)]
pub(super) struct ElementCache {
    bits: [Option<u32>; SIMD_WIDTH],
}

/// The law's value for element `element`, handed to `set` for it and, from element 0,
/// for the dead elements. Kept out of line so the unchanged-control path stays
/// small enough to inline.
#[inline(never)]
fn recompute<T>(
    element: usize,
    input: f32,
    dead: std::ops::Range<usize>,
    derive: &impl Fn(f32) -> T,
    set: &mut impl FnMut(usize, &T),
) {
    let value = derive(input);
    set(element, &value);
    if element == 0 {
        dead.for_each(|dead| set(dead, &value));
    }
}

impl ElementCache {
    /// Hands `set` the law's value for each of the first `active` elements whose
    /// input changed. Elements in `dead` follow element 0, so they compute on a
    /// valid operating point.
    #[inline(always)]
    pub(super) fn update<T>(
        &mut self,
        inputs: [f32; SIMD_WIDTH],
        active: usize,
        dead: std::ops::Range<usize>,
        derive: impl Fn(f32) -> T,
        mut set: impl FnMut(usize, &T),
    ) {
        let elements = self.bits.iter_mut().zip(inputs).enumerate();
        for (element, (cached, input)) in elements.take(active) {
            let bits = input.to_bits();
            if *cached != Some(bits) {
                *cached = Some(bits);
                recompute(element, input, dead.clone(), &derive, &mut set);
            }
        }
    }

    pub(super) fn clear_element(&mut self, element: usize) {
        self.bits[element] = None;
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_holds_positions_inside_the_table() {
        let table = [0.0, 1.0, 2.0, 3.0];
        let got = f32x4::lerp(&table, f32x4::new([1.5, -4.0, f32::INFINITY, 1.0e9])).to_array();
        assert_eq!(got[0], 1.5);
        assert_eq!(got[1], 0.0);
        assert!((got[2] - 3.0).abs() < 1.0e-3);
        assert!((got[3] - 3.0).abs() < 1.0e-3);
        assert_eq!(
            f32x4::lerp(&table, f32x4::splat(f32::NAN)).to_array(),
            [0.0; SIMD_WIDTH]
        );
        // Each element reads as the scalar lookup does.
        for (element, pos) in [1.5, -4.0, f32::INFINITY, 1.0e9].into_iter().enumerate() {
            assert_eq!(f32::lerp(&table, pos), got[element]);
        }
    }

    #[test]
    fn sanitize_clears_only_non_finite_elements() {
        let got = f32x4::new([1.0, f32::NAN, f32::NEG_INFINITY, -2.0])
            .sanitize()
            .to_array();
        assert_eq!(got, [1.0, 0.0, 0.0, -2.0]);
    }
}
