//! Plaits' LPC-10 speech synthesis: `LPCSpeechSynth`, a ten-pole lattice
//! filter excited by a sampled glottal pulse or noise, and the clock of
//! `LPCSpeechSynthController`, which runs it at 8 kHz times a formant ratio and
//! band-limits its steps onto the engine rate.

use super::polyblep::{edge_time, next_blep_sample, this_blep_sample};
use super::random::Random;
use super::tables::LUT_LPC_EXCITATION_PULSE;

const ORDER: usize = 10;

/// The glottal pulse, oversampled 32 times per synthesis tick.
const PULSE_SIZE: i32 = LUT_LPC_EXCITATION_PULSE.len() as i32;
const PULSE_OVERSAMPLING: f32 = 32.0;

/// The synthesis rate at a formant ratio of 1.
pub const SYNTHESIS_RATE: f32 = 8000.0;

/// One LPC-10 frame as Plaits stores it: energy, pitch period in ticks (0 for
/// an unvoiced frame) and the reflection coefficients, `k[0..2]` scaled by
/// 32768 and the rest by 128.
#[derive(Clone, Copy)]
pub struct Frame {
    energy: u8,
    period: u8,
    k: [i16; ORDER],
}

impl Frame {
    /// A frame from its twelve fields in Plaits' declaration order.
    pub fn from_fields(fields: &[i16]) -> Self {
        let mut k = [0; ORDER];
        k.copy_from_slice(&fields[2..2 + ORDER]);
        Self {
            energy: fields[0] as u8,
            period: fields[1] as u8,
            k,
        }
    }
}

#[inline(always)]
fn pulse(index: i32) -> f32 {
    f32::from(LUT_LPC_EXCITATION_PULSE[index.max(0) as usize]) / 128.0
}

/// `LPCSpeechSynth` with Plaits' prosody off: the pitch comes from the
/// controller alone.
#[derive(Clone, Copy, Default)]
pub struct LpcSynth {
    phase: f32,
    noise_energy: f32,
    pulse_energy: f32,
    next_sample: f32,
    /// Position in the glottal pulse; `PULSE_SIZE` or more once it has played.
    pulse_index: i32,
    k: [f32; ORDER],
    s: [f32; ORDER],
}

impl LpcSynth {
    /// Sets the filter and excitation `blend` of the way from `f1` to `f2`.
    pub fn play_frame(&mut self, f1: &Frame, f2: &Frame, blend: f32) {
        let energy_1 = f32::from(f1.energy) / 256.0;
        let energy_2 = f32::from(f2.energy) / 256.0;
        let noise_1 = if f1.period == 0 { energy_1 } else { 0.0 };
        let noise_2 = if f2.period == 0 { energy_2 } else { 0.0 };
        self.noise_energy = noise_1 + (noise_2 - noise_1) * blend;
        let pulse_1 = if f1.period != 0 { energy_1 } else { 0.0 };
        let pulse_2 = if f2.period != 0 { energy_2 } else { 0.0 };
        self.pulse_energy = pulse_1 + (pulse_2 - pulse_1) * blend;
        for (i, k) in self.k.iter_mut().enumerate() {
            let scale = if i < 2 { 32768.0 } else { 128.0 };
            let a = f32::from(f1.k[i]) / scale;
            let b = f32::from(f2.k[i]) / scale;
            *k = a + (b - a) * blend;
        }
    }

    /// The reversed glottal pulse at `phase`, `speed` cycles per tick from the
    /// next downward wrap: forward's pulse played from its end.
    #[inline(always)]
    fn reversed_pulse(&self, phase: f32, speed: f32) -> f32 {
        let index = (PULSE_OVERSAMPLING * (phase / speed)) as i32;
        if index < PULSE_SIZE {
            pulse(index) * self.pulse_energy
        } else {
            0.0
        }
    }

    /// One synthesis tick with the glottis at `f` cycles per tick (within
    /// ±0.5; negative plays the pulses reversed): the excitation and the
    /// filtered voice.
    #[inline(always)]
    pub fn tick(&mut self, f: f32, random: &mut Random) -> [f32; 2] {
        self.phase += f;
        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        if f >= 0.0 {
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                let reset_time = self.phase / f;
                let reset_sample = (PULSE_OVERSAMPLING * reset_time) as i32;
                let mut discontinuity = 0.0;
                if self.pulse_index < PULSE_SIZE {
                    self.pulse_index -= reset_sample;
                    discontinuity = pulse(self.pulse_index) * self.pulse_energy;
                }
                this_sample -= discontinuity * this_blep_sample(reset_time);
                next_sample -= discontinuity * next_blep_sample(reset_time);
                self.pulse_index = reset_sample;
            }
            if self.pulse_index < PULSE_SIZE {
                next_sample += pulse(self.pulse_index) * self.pulse_energy;
                self.pulse_index += PULSE_OVERSAMPLING as i32;
            }
        } else {
            let speed = -f;
            if self.phase < 0.0 {
                let t = edge_time(self.phase, f);
                self.phase += 1.0;
                if self.phase >= 1.0 {
                    self.phase = 0.0;
                }
                // The reversed pulse ends on its first value and restarts
                // where a forward one is cut off after a period.
                let discontinuity = self.reversed_pulse(1.0, speed) - pulse(0) * self.pulse_energy;
                this_sample += discontinuity * this_blep_sample(t);
                next_sample += discontinuity * next_blep_sample(t);
            }
            next_sample += self.reversed_pulse(self.phase, speed);
            // A forward pulse starts only at the next forward wrap.
            self.pulse_index = PULSE_SIZE;
        }
        self.next_sample = next_sample;

        let noise = if (random.word() >> 16) as u16 as i16 > 0 {
            self.noise_energy
        } else {
            -self.noise_energy
        };
        let excitation = (noise + this_sample) * 1.5;
        let k = &self.k;
        let s = &mut self.s;
        let mut e = [0.0f32; ORDER + 1];
        e[ORDER] = excitation;
        for i in (0..ORDER).rev() {
            e[i] = e[i + 1] - k[i] * s[i];
        }
        e[0] = e[0].clamp(-2.0, 2.0);
        for i in (1..ORDER).rev() {
            s[i] = s[i - 1] + k[i - 1] * e[i - 1];
        }
        s[0] = e[0];
        [excitation, e[0]]
    }
}

/// Most synthesis ticks one engine sample can hold: 8 kHz at the highest
/// formant ratio is under 3 per sample at an 8 kHz engine rate.
const MAX_TICKS: usize = 4;

/// The controller's clock: steps the synth at its own rate and band-limits
/// each new value onto the engine's samples.
#[derive(Clone, Copy, Default)]
pub struct LpcClock {
    phase: f32,
    sample: [f32; 2],
    next_sample: [f32; 2],
}

impl LpcClock {
    /// One engine sample with the synth ticking `rate` times per sample at
    /// `f` cycles per tick: the excitation and the voice.
    #[inline(always)]
    pub fn step(
        &mut self,
        rate: f32,
        f: f32,
        synth: &mut LpcSynth,
        random: &mut Random,
    ) -> [f32; 2] {
        let mut this_sample = self.next_sample;
        self.next_sample = [0.0; 2];
        self.phase += rate;
        for _ in 0..MAX_TICKS {
            if self.phase < 1.0 {
                break;
            }
            self.phase -= 1.0;
            let reset_time = self.phase / rate;
            let new_sample = synth.tick(f, random);
            for i in 0..2 {
                let discontinuity = new_sample[i] - self.sample[i];
                this_sample[i] += discontinuity * this_blep_sample(reset_time);
                self.next_sample[i] += discontinuity * next_blep_sample(reset_time);
            }
            self.sample = new_sample;
        }
        self.next_sample[0] += self.sample[0];
        self.next_sample[1] += self.sample[1];
        this_sample
    }
}
