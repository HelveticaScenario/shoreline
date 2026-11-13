//! One circuit, several widths. Each drum circuit is written once over
//! [`Simd`] and runs as [`Mono`], one lane in `f32` and `f64`, or as
//! [`X2`] or [`X4`], two or four lanes at once in vectors. Every element of
//! a vector circuit computes exactly what a `Mono` circuit computes for its
//! lane: the same operations in the same order with the same rounding,
//! so a lane sounds the same whichever width it runs at.
//!
//! Branches become masks: a vector circuit computes both sides where its
//! elements disagree and selects per element, and runs a loop until every element is
//! done, holding the elements that finish first.

use std::ops::{
    Add, AddAssign, BitAnd, BitOr, Div, DivAssign, Mul, MulAssign, Neg, Not, Sub, SubAssign,
};

mod vector;
mod x2;
mod x4;

pub use x2::X2;
pub use x4::X4;

/// Per-element flags: a `bool` for [`Mono`].
pub trait Mask:
    Copy + Default + BitAnd<Output = Self> + BitOr<Output = Self> + Not<Output = Self>
{
    fn any(self) -> bool;
    fn all(self) -> bool;
    /// Bit `l` set where element `l`'s flag is.
    fn bits(self) -> u32;

    /// `self & f()`, calling `f` only while some element is set, as `&&` does
    /// for one element.
    #[inline(always)]
    fn and_then(self, f: impl FnOnce() -> Self) -> Self {
        if self.any() { self & f() } else { self }
    }
}

impl Mask for bool {
    #[inline(always)]
    fn any(self) -> bool {
        self
    }
    #[inline(always)]
    fn all(self) -> bool {
        self
    }
    #[inline(always)]
    fn bits(self) -> u32 {
        self as u32
    }
}

/// A value per element: `f32` or `f64` for [`Mono`]. Arithmetic takes either
/// another value or a plain number, which applies to every element.
pub trait Real:
    Copy
    + Default
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + Add<Self::Elem, Output = Self>
    + Sub<Self::Elem, Output = Self>
    + Mul<Self::Elem, Output = Self>
    + Div<Self::Elem, Output = Self>
    + AddAssign
    + SubAssign
    + MulAssign
    + DivAssign
    + AddAssign<Self::Elem>
    + SubAssign<Self::Elem>
    + MulAssign<Self::Elem>
    + DivAssign<Self::Elem>
{
    /// The number in each element.
    type Elem: Copy;
    type Mask: Mask;

    fn splat(value: Self::Elem) -> Self;
    /// `self · a + b`, rounded once where the hardware fuses it.
    fn mul_add(self, a: Self, b: Self) -> Self;
    /// Ignores a NaN operand, as `f32::min` does.
    fn min(self, rhs: Self) -> Self;
    fn max(self, rhs: Self) -> Self;
    /// As `f32::clamp`: a NaN stays NaN.
    fn clamp(self, lo: Self, hi: Self) -> Self;
    fn abs(self) -> Self;
    fn floor(self) -> Self;
    /// As [`sanitize`](crate::dsp::utils::sanitize): non-finite elements
    /// become 0.
    fn sanitize(self) -> Self;
    /// As `f32::fract`: `self` less its integer part.
    fn fract(self) -> Self;
    fn lt(self, rhs: Self) -> Self::Mask;
    fn gt(self, rhs: Self) -> Self::Mask;
    fn ge(self, rhs: Self) -> Self::Mask;
    fn select(mask: Self::Mask, if_true: Self, if_false: Self) -> Self;
    /// `f` applied element by element.
    fn map(self, f: impl Fn(Self::Elem) -> Self::Elem) -> Self;
    /// `f` applied to the elements in `mask`; the others keep `keep`'s.
    fn map_where(self, mask: Self::Mask, keep: Self, f: impl Fn(Self::Elem) -> Self::Elem) -> Self;
}

/// `value` in every element, its type taken from where it goes:
/// `x.max(c(0.0))`.
#[inline(always)]
pub fn c<R: Real>(value: R::Elem) -> R {
    R::splat(value)
}

/// `a·b + c`, fused where the hardware has it and as two roundings
/// otherwise, never a library call.
macro_rules! fast_fma {
    ($a:expr, $b:expr, $c:expr) => {{
        #[cfg(any(target_arch = "aarch64", target_feature = "fma"))]
        {
            $a.mul_add($b, $c)
        }
        #[cfg(not(any(target_arch = "aarch64", target_feature = "fma")))]
        {
            $a * $b + $c
        }
    }};
}

macro_rules! scalar_real {
    ($t:ty) => {
        impl Real for $t {
            type Elem = $t;
            type Mask = bool;

            #[inline(always)]
            fn splat(value: $t) -> Self {
                value
            }
            #[inline(always)]
            fn mul_add(self, a: Self, b: Self) -> Self {
                fast_fma!(self, a, b)
            }
            #[inline(always)]
            fn min(self, rhs: Self) -> Self {
                <$t>::min(self, rhs)
            }
            #[inline(always)]
            fn max(self, rhs: Self) -> Self {
                <$t>::max(self, rhs)
            }
            #[inline(always)]
            fn clamp(self, lo: Self, hi: Self) -> Self {
                <$t>::clamp(self, lo, hi)
            }
            #[inline(always)]
            fn abs(self) -> Self {
                <$t>::abs(self)
            }
            #[inline(always)]
            fn floor(self) -> Self {
                <$t>::floor(self)
            }
            #[inline(always)]
            fn sanitize(self) -> Self {
                if self.is_finite() { self } else { 0.0 }
            }
            #[inline(always)]
            fn fract(self) -> Self {
                <$t>::fract(self)
            }
            #[inline(always)]
            fn lt(self, rhs: Self) -> bool {
                self < rhs
            }
            #[inline(always)]
            fn gt(self, rhs: Self) -> bool {
                self > rhs
            }
            #[inline(always)]
            fn ge(self, rhs: Self) -> bool {
                self >= rhs
            }
            #[inline(always)]
            fn select(mask: bool, if_true: Self, if_false: Self) -> Self {
                if mask { if_true } else { if_false }
            }
            #[inline(always)]
            fn map(self, f: impl Fn($t) -> $t) -> Self {
                f(self)
            }
            #[inline(always)]
            fn map_where(self, mask: bool, keep: Self, f: impl Fn($t) -> $t) -> Self {
                if mask { f(self) } else { keep }
            }
        }
    };
}

scalar_real!(f32);
scalar_real!(f64);

/// How wide a circuit runs: its single- and double-precision values, their
/// masks, and which of a network's switches are closed.
pub trait Simd: Copy + Default + 'static {
    type S: Real<Elem = f32, Mask = Self::SM> + Elementwise<Element = f32>;
    type D: Real<Elem = f64, Mask = Self::DM> + Elementwise<Element = f64>;
    type SM: Mask + Elementwise<Element = bool>;
    type DM: Mask + Elementwise<Element = bool>;
    /// Per element, the index of the combination of switches closed in a
    /// network: bit `s` for switch `s`.
    type Closed: Copy;
    /// Lanes at once.
    const WIDTH: usize;

    fn wide(s: Self::S) -> Self::D;
    fn narrow(d: Self::D) -> Self::S;
    fn wide_mask(m: Self::SM) -> Self::DM;
    fn narrow_mask(m: Self::DM) -> Self::SM;
    /// The switch combination with switch `s` closed where `closed[s]` is.
    fn closed<const S: usize>(closed: [Self::SM; S]) -> Self::Closed;
    /// The combination every element has, if they agree.
    fn uniform(closed: Self::Closed) -> Option<usize>;
    /// Element `element`'s combination.
    fn closed_element(closed: Self::Closed, element: usize) -> usize;
    /// Where switch `s` is closed.
    fn switch_on(closed: Self::Closed, s: usize) -> Self::SM;
    /// A value gathered element by element from `f`.
    fn gather(f: impl Fn(usize) -> f32) -> Self::S;
    /// Element `l` set where bit `l` of `bits` is.
    fn mask(bits: u32) -> Self::SM;
}

/// One lane.
#[derive(Clone, Copy, Default)]
pub struct Mono;

impl Simd for Mono {
    type S = f32;
    type D = f64;
    type SM = bool;
    type DM = bool;
    type Closed = usize;
    const WIDTH: usize = 1;

    #[inline(always)]
    fn wide(s: f32) -> f64 {
        s as f64
    }
    #[inline(always)]
    fn narrow(d: f64) -> f32 {
        d as f32
    }
    #[inline(always)]
    fn wide_mask(m: bool) -> bool {
        m
    }
    #[inline(always)]
    fn narrow_mask(m: bool) -> bool {
        m
    }
    #[inline(always)]
    fn closed<const S: usize>(closed: [bool; S]) -> usize {
        closed
            .iter()
            .enumerate()
            .fold(0, |k, (s, &on)| k | ((on as usize) << s))
    }
    #[inline(always)]
    fn uniform(closed: usize) -> Option<usize> {
        Some(closed)
    }
    #[inline(always)]
    fn closed_element(closed: usize, _element: usize) -> usize {
        closed
    }
    #[inline(always)]
    fn switch_on(closed: usize, s: usize) -> bool {
        closed & (1 << s) != 0
    }
    #[inline(always)]
    fn gather(f: impl Fn(usize) -> f32) -> f32 {
        f(0)
    }
    #[inline(always)]
    fn mask(bits: u32) -> bool {
        bits & 1 != 0
    }
}

/// A group's state seen one element at a time, as the [`Mono`] form of the
/// same state. Hits start, and lanes move between widths, a element at a
/// time.
pub trait Elementwise {
    type Element;
    fn element(&self, element: usize) -> Self::Element;
    fn set_element(&mut self, element: usize, value: &Self::Element);
}

macro_rules! scalar_elementwise {
    ($($t:ty),*) => {$(
        impl Elementwise for $t {
            type Element = $t;
            #[inline(always)]
            fn element(&self, _element: usize) -> $t {
                *self
            }
            #[inline(always)]
            fn set_element(&mut self, _element: usize, value: &$t) {
                *self = *value;
            }
        }
    )*};
}

scalar_elementwise!(f32, f64, bool, ());

impl<T: Elementwise, const N: usize> Elementwise for [T; N] {
    type Element = [T::Element; N];
    #[inline]
    fn element(&self, element: usize) -> Self::Element {
        std::array::from_fn(|k| self[k].element(element))
    }
    #[inline]
    fn set_element(&mut self, element: usize, value: &Self::Element) {
        for (v, x) in self.iter_mut().zip(value) {
            v.set_element(element, x);
        }
    }
}

/// Implements [`Elementwise`] for `$ty<L>` over the listed fields, which must
/// be all of them: the [`Mono`] form's literal names every field.
macro_rules! elementwise {
    ($ty:ident { $($field:ident),* $(,)? }) => {
        impl<L: $crate::dsp::drums::simd::Simd> $crate::dsp::drums::simd::Elementwise for $ty<L> {
            type Element = $ty<$crate::dsp::drums::simd::Mono>;
            #[inline]
            fn element(&self, element: usize) -> Self::Element {
                $ty { $($field: $crate::dsp::drums::simd::Elementwise::element(&self.$field, element)),* }
            }
            #[inline]
            fn set_element(&mut self, element: usize, value: &Self::Element) {
                $($crate::dsp::drums::simd::Elementwise::set_element(&mut self.$field, element, &value.$field);)*
            }
        }
    };
}
pub(crate) use elementwise;
