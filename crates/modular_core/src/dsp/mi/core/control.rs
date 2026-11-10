//! `stmlib::ParameterInterpolator` without the block. Plaits ramps its controls
//! across 12-sample blocks; the ports set new targets every sample and ramp
//! over that one sample, so a band-limited restart can still read a control
//! between two samples.

/// `stmlib::ParameterInterpolator` without the block: a linear ramp from the
/// value it has reached to the last target set.
#[derive(Clone, Copy, Default)]
pub struct Ramp {
    value: f32,
    increment: f32,
}

impl Ramp {
    /// Ramps to `target` over `length` samples, or jumps there when
    /// `length` is `None` (a voice's first control). A NaN target holds the ramp
    /// where it is.
    #[inline]
    pub fn set(&mut self, target: f32, length: Option<f32>) {
        let target = if target.is_nan() { self.value } else { target };
        match length {
            Some(length) => self.increment = (target - self.value) / length,
            None => {
                self.value = target;
                self.increment = 0.0;
            }
        }
    }

    /// Whether the ramp has reached its target and stays there.
    #[inline(always)]
    pub fn settled(&self) -> bool {
        self.increment == 0.0
    }

    #[inline(always)]
    pub fn next_value(&mut self) -> f32 {
        self.value += self.increment;
        self.value
    }

    /// The value `t` samples past the last [`next_value`](Self::next_value).
    #[inline(always)]
    pub fn subsample(&self, t: f32) -> f32 {
        self.value + self.increment * t
    }
}
