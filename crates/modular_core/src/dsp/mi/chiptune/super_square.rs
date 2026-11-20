//! `plaits::SuperSquareOscillator`: a square restarted by a master oscillator
//! at the note, so a slower square narrows to a pulse and a faster one
//! hard-syncs.

use crate::dsp::mi::core::polyblep::{edge_time, next_blep_sample, this_blep_sample};

#[derive(Clone, Copy, Default)]
pub struct SuperSquare {
    master_phase: f32,
    slave_phase: f32,
    next_sample: f32,
    high: bool,
}

impl SuperSquare {
    /// One sample at `frequency` cycles per sample (0 to `max`, `max` at most
    /// 0.5) and `shape` (0–1): pulses narrowing toward 0 below 0.5, a square
    /// at 0.5, rising hard sync above it.
    #[inline(always)]
    pub fn step(&mut self, frequency: f32, shape: f32, max: f32) -> f32 {
        let ratio = if shape < 0.5 {
            0.51 + 0.98 * shape
        } else {
            1.0 + 16.0 * (shape - 0.5) * (shape - 0.5)
        };
        let master_frequency = frequency.min(max);
        let slave_frequency = (frequency * ratio).min(max);

        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        let mut reset = false;
        let mut transition_during_reset = false;
        let mut reset_time = 0.0;

        self.master_phase += master_frequency;
        if self.master_phase >= 1.0 {
            self.master_phase -= 1.0;
            reset_time = edge_time(self.master_phase, master_frequency);
            let mut slave_phase_at_reset = self.slave_phase + (1.0 - reset_time) * slave_frequency;
            reset = true;
            if slave_phase_at_reset >= 1.0 {
                slave_phase_at_reset -= 1.0;
                transition_during_reset = true;
            }
            if !self.high && slave_phase_at_reset >= 0.5 {
                transition_during_reset = true;
            }
            let value = if slave_phase_at_reset < 0.5 { 0.0 } else { 1.0 };
            this_sample -= value * this_blep_sample(reset_time);
            next_sample -= value * next_blep_sample(reset_time);
        }

        self.slave_phase += slave_frequency;
        // At most half a cycle per sample, one pass takes both edges a
        // sample can hold, and the second finds none left.
        for _ in 0..2 {
            if reset && !transition_during_reset {
                break;
            }
            if !self.high {
                if self.slave_phase < 0.5 {
                    break;
                }
                let t = edge_time(self.slave_phase - 0.5, slave_frequency);
                this_sample += this_blep_sample(t);
                next_sample += next_blep_sample(t);
                self.high = true;
            }
            if self.high {
                if self.slave_phase < 1.0 {
                    break;
                }
                self.slave_phase -= 1.0;
                let t = edge_time(self.slave_phase, slave_frequency);
                this_sample -= this_blep_sample(t);
                next_sample -= next_blep_sample(t);
                self.high = false;
            }
        }

        if reset {
            self.slave_phase = reset_time * slave_frequency;
            self.high = false;
        }

        next_sample += if self.slave_phase < 0.5 { 0.0 } else { 1.0 };
        self.next_sample = next_sample;
        2.0 * this_sample - 1.0
    }
}
