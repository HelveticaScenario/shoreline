//! `plaits::Downsampler`: a 4× decimator whose 8-tap FIR spans two output
//! samples.

use super::tables::LUT_4X_DOWNSAMPLER_FIR;

/// Oversampling factor of [`Downsampler`].
pub const OVERSAMPLING: usize = 4;

#[derive(Clone, Copy, Default)]
pub struct Downsampler {
    head: f32,
    tail: f32,
}

impl Downsampler {
    /// Adds substep `i` (0–3) of the current output sample.
    #[inline(always)]
    pub fn accumulate(&mut self, i: usize, sample: f32) {
        self.head += sample * LUT_4X_DOWNSAMPLER_FIR[3 - i];
        self.tail += sample * LUT_4X_DOWNSAMPLER_FIR[i];
    }

    /// The finished output sample, after all four substeps.
    #[inline(always)]
    pub fn read(&mut self) -> f32 {
        let value = self.head;
        self.head = self.tail;
        self.tail = 0.0;
        value
    }
}
