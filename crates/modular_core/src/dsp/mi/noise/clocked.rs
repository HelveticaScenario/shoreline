//! Plaits' `ClockedNoise`: white noise held between ticks of a clock, with
//! band-limited steps, fading into the white noise itself as the clock nears
//! a quarter of the 48 kHz rate.

use crate::dsp::mi::core::polyblep::{next_blep_sample, this_blep_sample};

#[derive(Clone, Copy, Default)]
pub struct ClockedNoise {
    phase: f32,
    sample: f32,
    next_sample: f32,
}

impl ClockedNoise {
    /// One sample at `frequency` cycles per sample (0–1). `white` is this
    /// sample's draw, from −1 to 1; the held value steps to it on a tick.
    /// `raw_amount` blends in `raw`, the same draw scaled for the rate.
    #[inline(always)]
    pub fn step(
        &mut self,
        sync: bool,
        frequency: f32,
        white: f32,
        raw: f32,
        raw_amount: f32,
    ) -> f32 {
        if sync {
            self.phase = 1.0;
        }
        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        self.phase += frequency;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            let t = self.phase / frequency;
            let discontinuity = white - self.sample;
            this_sample += discontinuity * this_blep_sample(t);
            next_sample += discontinuity * next_blep_sample(t);
            self.sample = white;
        }
        self.next_sample = next_sample + self.sample;
        this_sample + raw_amount * (raw - this_sample)
    }
}
