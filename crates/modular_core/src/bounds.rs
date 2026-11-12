//! The values a signal swings between, carried beside its samples.
//!
//! A bounded output stores one [`Bounds`] per channel per slot, written with
//! the sample it describes and read through the same paths, so a reader at
//! slot `i` always sees the bounds of slot `i`. Whether an output is bounded
//! is settled when the patch is built (see [`BoundsKind`]); an unbounded
//! output stores nothing and reads [`Bounds::UNBOUNDED`].

/// The values a signal takes at the start and end of its swing, in that
/// order: a signal remapped to `(5, 0)` falls as its source rises, and a
/// reader mapping from these bounds maps 5 to its own start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub start: f32,
    pub end: f32,
}

impl Default for Bounds {
    fn default() -> Self {
        Self::UNBOUNDED
    }
}

impl Bounds {
    pub const UNBOUNDED: Self = Self {
        start: f32::NEG_INFINITY,
        end: f32::INFINITY,
    };

    #[inline]
    pub const fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }

    /// A signal that holds `value`.
    #[inline]
    pub const fn point(value: f32) -> Self {
        Self::new(value, value)
    }

    #[inline]
    pub fn is_bounded(self) -> bool {
        self.start.is_finite() && self.end.is_finite()
    }

    #[inline]
    pub fn is_point(self) -> bool {
        self.start == self.end
    }

    /// Whether these bounds make a range a signal swings over: bounded, with
    /// ends that differ. A signal that holds one value has bounds but no
    /// range to map from or vary within.
    #[inline]
    pub fn swings(self) -> bool {
        self.is_bounded() && !self.is_point()
    }

    /// The lower end.
    #[inline]
    pub fn lo(self) -> f32 {
        self.start.min(self.end)
    }

    /// The upper end.
    #[inline]
    pub fn hi(self) -> f32 {
        self.start.max(self.end)
    }

    /// These bounds with the ends in rising order.
    #[inline]
    pub fn sorted(self) -> Self {
        Self::new(self.lo(), self.hi())
    }

    /// Both ends through `f`, keeping their order: the bounds of `f(x)` for
    /// an `f` that never falls as `x` rises, or that never rises (which
    /// swaps the swing, as `f` does).
    #[inline]
    pub fn map(self, f: impl Fn(f32) -> f32) -> Self {
        Self::new(f(self.start), f(self.end))
    }

    /// The bounds of `f(x, y)` for `x` within `self` and `y` within `other`,
    /// where `f` is monotonic in each argument. When either side holds still
    /// the ends map through `f` in order, so orientation follows the
    /// operation (`x * -2` swaps the swing); otherwise the result spans the
    /// four corners, rising.
    #[inline]
    pub fn combine(self, other: Self, f: impl Fn(f32, f32) -> f32) -> Self {
        if other.is_point() {
            return self.map(|x| f(x, other.start));
        }
        if self.is_point() {
            return other.map(|y| f(self.start, y));
        }
        let corners = [
            f(self.start, other.start),
            f(self.start, other.end),
            f(self.end, other.start),
            f(self.end, other.end),
        ];
        let lo = corners.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = corners.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        Self::new(lo, hi)
    }

    /// The smallest bounds holding both, rising.
    #[inline]
    pub fn hull(self, other: Self) -> Self {
        Self::new(self.lo().min(other.lo()), self.hi().max(other.hi()))
    }
}

/// How a cable reads its producer's bounds, resolved at `connect()`.
#[derive(Clone, Copy, Debug, Default)]
pub enum PortBounds {
    /// The port is never bounded.
    #[default]
    Unbounded,
    /// The port's declared range.
    Fixed(Bounds),
    /// The producer's stored bounds, valid below its `processed` cursor.
    /// Points into the producer's block buffer under the same lifetime
    /// invariant as a [`CableView`](crate::types::CableView).
    Stored {
        store: *const crate::block_port::BoundsStore,
        processed: *const std::cell::Cell<usize>,
    },
    /// Stored by a Sample-mode producer, whose single-slot reads go through
    /// [`get_bounds_at`](crate::types::Sampleable::get_bounds_at) to drive
    /// its processing and the feedback delay. A Block-mode reader, never
    /// re-entered, reads whole ranges in place once rendered.
    Live {
        store: *const crate::block_port::BoundsStore,
        processed: *const std::cell::Cell<usize>,
    },
}

// SAFETY: built during `connect()` on the audio thread and only read there; it
// crosses threads only as part of params, like `Signal`.
unsafe impl Send for PortBounds {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_operand_maps_the_ends_in_order() {
        let swing = Bounds::new(5.0, 0.0);
        assert_eq!(
            swing.combine(Bounds::point(2.0), |x, y| x + y),
            Bounds::new(7.0, 2.0)
        );
        assert_eq!(
            swing.combine(Bounds::point(-1.0), |x, y| x * y),
            Bounds::new(-5.0, 0.0)
        );
        assert_eq!(
            Bounds::point(5.0).combine(Bounds::new(1.0, 3.0), |x, y| x - y),
            Bounds::new(4.0, 2.0)
        );
    }

    #[test]
    fn two_swings_span_the_corners() {
        let a = Bounds::new(-1.0, 2.0);
        let b = Bounds::new(3.0, -2.0);
        assert_eq!(a.combine(b, |x, y| x * y), Bounds::new(-4.0, 6.0));
    }

    #[test]
    fn hull_and_sorting_rise() {
        assert_eq!(Bounds::new(3.0, 1.0).sorted(), Bounds::new(1.0, 3.0));
        assert_eq!(
            Bounds::new(3.0, 1.0).hull(Bounds::new(-2.0, 0.0)),
            Bounds::new(-2.0, 3.0)
        );
        assert!(!Bounds::UNBOUNDED.is_bounded());
        assert!(Bounds::point(0.0).is_bounded());
    }
}
