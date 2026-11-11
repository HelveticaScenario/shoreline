//! Plaits' DX7 LFO (`fm/lfo.h`), stepped every sample, with its delayed
//! fade-in. Its rates are cycles per sample.

use super::super::core::{random::Random, sine::sine};
use super::patch::LfoShape;

const TRIANGLE: u8 = 0;
const RAMP_DOWN: u8 = 1;
const RAMP_UP: u8 = 2;
const SQUARE: u8 = 3;
const SINE: u8 = 4;

#[derive(Clone, Copy, Debug)]
pub struct Lfo {
    phase: f32,
    frequency: f32,
    delay_phase: f32,
    delay_increment: [f32; 2],
    value: f32,
    random_value: f32,
    amp_mod_depth: f32,
    pitch_mod_depth: f32,
    waveform: u8,
    reset_phase: bool,
    phase_integral: i32,
}

/// `Lfo::Init`: a voice's LFO before any patch.
impl Default for Lfo {
    fn default() -> Self {
        Self {
            phase: 0.0,
            frequency: 0.1,
            delay_phase: 0.0,
            delay_increment: [0.1; 2],
            value: 0.0,
            random_value: 0.0,
            amp_mod_depth: 0.0,
            pitch_mod_depth: 0.0,
            waveform: TRIANGLE,
            reset_phase: false,
            phase_integral: 0,
        }
    }
}

impl Lfo {
    /// Takes a patch's settings, at `one_hz` cycles per sample per Hz.
    pub fn set(&mut self, shape: &LfoShape, one_hz: f32) {
        self.frequency = shape.frequency * one_hz;
        self.delay_increment = shape.delay.map(|d| d * one_hz);
        self.waveform = shape.waveform;
        self.reset_phase = shape.reset_phase;
        self.amp_mod_depth = shape.amp_mod_depth;
        self.pitch_mod_depth = shape.pitch_mod_depth;
    }

    /// A note-on: restarts the delay, and the phase if the patch asks.
    pub fn reset(&mut self) {
        if self.reset_phase {
            self.phase = 0.0;
        }
        self.delay_phase = 0.0;
    }

    /// One sample; a new random value at each wrap.
    #[inline(always)]
    pub fn step(&mut self, random: &mut Random) {
        self.phase += self.frequency;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.random_value = random.float();
        }
        self.value = self.value();
        self.delay_phase += self.delay_increment[usize::from(self.delay_phase >= 0.5)];
        if self.delay_phase >= 1.0 {
            self.delay_phase = 1.0;
        }
    }

    /// Jumps to `sample` samples after a note-on; a new random value whenever
    /// that crosses into another cycle.
    #[inline(always)]
    pub fn scrub(&mut self, mut sample: f32, random: &mut Random) {
        let phase = sample * self.frequency;
        let integral = phase as i32;
        self.phase = phase - integral as f32;
        if integral != self.phase_integral {
            self.phase_integral = integral;
            self.random_value = random.float();
        }
        self.value = self.value();
        self.delay_phase = sample * self.delay_increment[0];
        if self.delay_phase > 0.5 {
            sample -= 0.5 / self.delay_increment[0];
            self.delay_phase = 0.5 + sample * self.delay_increment[1];
            if self.delay_phase >= 1.0 {
                self.delay_phase = 1.0;
            }
        }
    }

    fn value(&self) -> f32 {
        let phase = self.phase;
        match self.waveform {
            TRIANGLE => {
                2.0 * if phase < 0.5 {
                    0.5 - phase
                } else {
                    phase - 0.5
                }
            }
            RAMP_DOWN => 1.0 - phase,
            RAMP_UP => phase,
            SQUARE => {
                if phase < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
            SINE => 0.5 + 0.5 * sine(phase + 0.5),
            _ => self.random_value,
        }
    }

    fn delay_ramp(&self) -> f32 {
        if self.delay_phase < 0.5 {
            0.0
        } else {
            (self.delay_phase - 0.5) * 2.0
        }
    }

    /// Pitch modulation, in octaves.
    pub fn pitch_mod(&self) -> f32 {
        (self.value - 0.5) * self.delay_ramp() * self.pitch_mod_depth
    }

    pub fn amp_mod(&self) -> f32 {
        (1.0 - self.value) * self.delay_ramp() * self.amp_mod_depth
    }
}
