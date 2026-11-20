//! `plaits::NESTriangleOscillator<5>`: the NES's 32-step triangle, fading
//! into a band-limited triangle where its steps would alias.

use crate::dsp::mi::core::{
    control::Ramp,
    polyblep::{
        edge_time, next_blep_sample, next_integrated_blep_sample, this_blep_sample,
        this_integrated_blep_sample,
    },
};

const NUM_STEPS: i32 = 32;
const HALF: i32 = NUM_STEPS / 2;
const TOP: i32 = NUM_STEPS - 1;
const NUM_STEPS_F: f32 = NUM_STEPS as f32;
const SCALE: f32 = 4.0 / (TOP - 1) as f32;

#[derive(Clone, Copy)]
pub struct NesTriangle {
    phase: f32,
    next_sample: f32,
    step: i32,
    ascending: bool,
    frequency: Ramp,
}

impl Default for NesTriangle {
    fn default() -> Self {
        let mut frequency = Ramp::default();
        frequency.set(0.001, None);
        Self {
            phase: 0.0,
            next_sample: 0.0,
            step: 0,
            ascending: true,
            frequency,
        }
    }
}

impl NesTriangle {
    /// One sample at `frequency` cycles per sample (0 to `max`, `max` at most
    /// 0.5).
    #[inline(always)]
    pub fn step(&mut self, frequency: f32, max: f32) -> f32 {
        self.frequency.set(frequency.min(max), Some(1.0));
        let frequency = self.frequency.next_value();
        self.phase += frequency;

        let fade_to_tri = ((frequency - 0.5 / NUM_STEPS_F) * 2.0 * NUM_STEPS_F).clamp(0.0, 1.0);
        let nes_gain = 1.0 - fade_to_tri;
        let tri_gain = fade_to_tri * 2.0 / SCALE;

        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;

        if self.ascending && self.phase >= 0.5 {
            let discontinuity = 4.0 * frequency * tri_gain;
            if discontinuity != 0.0 {
                let t = edge_time(self.phase - 0.5, frequency);
                this_sample -= this_integrated_blep_sample(t) * discontinuity;
                next_sample -= next_integrated_blep_sample(t) * discontinuity;
            }
            self.ascending = false;
        }

        let mut next_step = (self.phase * NUM_STEPS_F) as i32;
        if next_step != self.step {
            let mut wrap = false;
            if next_step >= NUM_STEPS {
                self.phase -= 1.0;
                next_step -= NUM_STEPS;
                wrap = true;
            }

            let mut discontinuity = if next_step < HALF { 1.0 } else { -1.0 };
            if next_step == 0 || next_step == HALF {
                discontinuity = 0.0;
            }

            discontinuity *= nes_gain;
            if discontinuity != 0.0 {
                let fraction = self.phase * NUM_STEPS_F - next_step as f32;
                let t = edge_time(fraction, frequency * NUM_STEPS_F);
                this_sample += this_blep_sample(t) * discontinuity;
                next_sample += next_blep_sample(t) * discontinuity;
            }

            if wrap {
                let discontinuity = 4.0 * frequency * tri_gain;
                if discontinuity != 0.0 {
                    let t = edge_time(self.phase, frequency);
                    this_sample += this_integrated_blep_sample(t) * discontinuity;
                    next_sample += next_integrated_blep_sample(t) * discontinuity;
                }
                self.ascending = true;
            }
        }
        self.step = next_step;

        next_sample += nes_gain
            * (if self.step < HALF {
                self.step
            } else {
                TOP - self.step
            }) as f32;
        next_sample += tri_gain
            * if self.phase < 0.5 {
                2.0 * self.phase
            } else {
                2.0 - 2.0 * self.phase
            };

        self.next_sample = next_sample;
        this_sample * SCALE - 1.0
    }
}
