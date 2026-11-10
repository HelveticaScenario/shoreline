//! `plaits::SwarmVoice`: one voice of the swarm, a grain envelope that
//! wanders its pitch, sounding a saw and a sine.

use crate::dsp::mi::core::{
    fast_sine::QuadratureOscillator,
    polyblep::{next_blep_sample, this_blep_sample},
    random::Random,
    sine::sine,
    units::semitones_to_ratio,
};

/// `plaits::GrainEnvelope`, stepped once per spectrum tick (Plaits' block):
/// a phase that runs from grain to grain, each grain drawing a new pitch
/// target and length.
#[derive(Clone, Copy)]
pub struct GrainEnvelope {
    from: f32,
    interval: f32,
    phase: f32,
    fm: f32,
    amplitude: f32,
    previous_size_ratio: f32,
    filter_coefficient: f32,
}

impl Default for GrainEnvelope {
    fn default() -> Self {
        Self {
            from: 0.0,
            interval: 1.0,
            phase: 1.0,
            fm: 0.0,
            amplitude: 0.5,
            previous_size_ratio: 0.0,
            filter_coefficient: 0.0,
        }
    }
}

impl GrainEnvelope {
    /// One tick at `rate` grains per tick. A burst restarts the grain and
    /// speeds its successors up.
    pub fn step(&mut self, rate: f32, burst_mode: bool, start_burst: bool, random: &mut Random) {
        let mut randomize = false;
        if start_burst {
            self.phase = 0.5;
            self.fm = 16.0;
            randomize = true;
        } else {
            self.phase += rate * self.fm;
            if self.phase >= 1.0 {
                self.phase -= (self.phase as i32) as f32;
                randomize = true;
            }
        }
        if randomize {
            self.from += self.interval;
            self.interval = random.float() - self.from;
            self.fm = if burst_mode {
                self.fm * (0.8 + 0.2 * random.float())
            } else {
                0.5 + 1.5 * random.float()
            };
        }
    }

    /// Where the pitch sits, −1 to 1: gliding between grains' targets below
    /// a `size_ratio` of 1, holding each grain's target from 1 up.
    pub fn frequency(&self, size_ratio: f32) -> f32 {
        if size_ratio < 1.0 {
            2.0 * (self.from + self.interval * self.phase) - 1.0
        } else {
            self.from
        }
    }

    /// The grain's level for this tick: a window shorter as `size_ratio`
    /// grows from 1, steady below it, eased across the switch between them.
    pub fn amplitude(&mut self, size_ratio: f32) -> f32 {
        let mut target = 1.0;
        if size_ratio >= 1.0 {
            let phase = ((self.phase - 0.5) * size_ratio).clamp(-1.0, 1.0);
            let e = sine(0.5 * phase + 1.25);
            target = 0.5 * (e + 1.0);
        }
        if (size_ratio >= 1.0) ^ (self.previous_size_ratio >= 1.0) {
            self.filter_coefficient = 0.5;
        }
        self.filter_coefficient *= 0.95;
        self.previous_size_ratio = size_ratio;
        self.amplitude += (0.5 - self.filter_coefficient) * (target - self.amplitude);
        self.amplitude
    }
}

/// `plaits::AdditiveSawOscillator`: a BLEP saw from −1 to 1, one sample
/// late. A negative frequency runs it backward, its edge mirrored.
#[derive(Clone, Copy, Default)]
pub struct Saw {
    phase: f32,
    next_sample: f32,
}

impl Saw {
    /// One sample at `frequency` cycles per sample (within ±0.5).
    #[inline(always)]
    pub fn step(&mut self, frequency: f32) -> f32 {
        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        self.phase += frequency;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            let t = self.phase / frequency;
            this_sample -= this_blep_sample(t);
            next_sample -= next_blep_sample(t);
        } else if self.phase < 0.0 {
            self.phase += 1.0;
            let t = (1.0 - self.phase) / -frequency;
            this_sample += this_blep_sample(t);
            next_sample += next_blep_sample(t);
        }
        next_sample += self.phase;
        self.next_sample = next_sample;
        2.0 * this_sample - 1.0
    }
}

/// A linear ramp across a tick, as Plaits' interpolators ramp across a
/// block.
#[derive(Clone, Copy, Default)]
pub struct TickRamp {
    pub value: f32,
    increment: f32,
}

impl TickRamp {
    /// Ramps to `target` over `period` samples, or jumps there when `jump`.
    pub fn set(&mut self, target: f32, period: f32, jump: bool) {
        if jump {
            self.value = target;
            self.increment = 0.0;
        } else {
            self.increment = (target - self.value) / period;
        }
    }

    #[inline(always)]
    pub fn next(&mut self) -> f32 {
        self.value += self.increment;
        self.value
    }
}

/// One voice: its envelope, its pitch ratio and level ramping across each
/// tick, and its two oscillators.
#[derive(Clone, Copy, Default)]
pub struct SwarmVoice {
    pub envelope: GrainEnvelope,
    pub ratio: TickRamp,
    pub level: TickRamp,
    pub saw: Saw,
    pub sine: QuadratureOscillator,
}

impl SwarmVoice {
    #[cfg(test)]
    pub fn envelope_phase(&self) -> f32 {
        self.envelope.phase
    }

    /// The voice's pitch ratio and level for a tick: the envelope's pitch,
    /// spread by `rank` (−1 to 4/3 across the swarm), and its grain level.
    pub fn targets(&mut self, rank: f32, spread: f32, size_ratio: f32) -> (f32, f32) {
        let level = self.envelope.amplitude(size_ratio) * (1.0 / super::VOICES as f32);
        let expo_amount = self.envelope.frequency(size_ratio);
        let ratio = semitones_to_ratio(48.0 * expo_amount * spread * rank)
            * (1.0 + rank * (rank + 0.01) * spread * 0.25);
        (ratio, level)
    }
}
