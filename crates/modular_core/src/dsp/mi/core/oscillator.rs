//! `plaits::Oscillator`: the slope shape on its through-zero path, a
//! band-limited triangle whose peak sits at `pw` of the cycle, and the square
//! and saw shapes on its plain path.

use super::control::Ramp;
use super::dsp::constrain;
use super::phase::wrap;
use super::polyblep::{
    edge_time, next_blep_sample, next_integrated_blep_sample, this_blep_sample,
    this_integrated_blep_sample,
};

#[derive(Clone, Copy)]
pub struct SlopeOscillator {
    phase: f32,
    next_sample: f32,
    /// Whether the phase is below the peak, on the rising slope.
    high: bool,
    /// The phase offset travelled so far, in `[0, 1)`.
    offset: f32,
    /// Plaits' one-sample `ParameterInterpolator`s, which land a hair off
    /// their target; kept so the oscillator matches Plaits bit for bit.
    frequency: Ramp,
    pw: Ramp,
}

impl Default for SlopeOscillator {
    fn default() -> Self {
        let mut frequency = Ramp::default();
        frequency.set(0.001, None);
        let mut pw = Ramp::default();
        pw.set(0.5, None);
        Self {
            phase: 0.5,
            next_sample: 0.0,
            high: true,
            offset: 0.0,
            frequency,
            pw,
        }
    }
}

impl SlopeOscillator {
    /// Starts a fresh oscillator `offset` (in `[0, 1)`) of a cycle further into
    /// its waveform, for a first step at `frequency` with the peak at `pw`.
    pub fn seed(&mut self, offset: f32, frequency: f32, pw: f32) {
        self.phase = wrap(self.phase + offset);
        self.offset = offset;
        // The peak where the first step puts it, from the same ramps.
        let (mut f, mut w) = (self.frequency, self.pw);
        f.set(frequency, Some(1.0));
        w.set(pw, Some(1.0));
        let pw = width(w.next_value(), f.next_value());
        // A phase on the peak has yet to cross it.
        self.high = self.phase <= pw;
    }

    /// One sample, from −1 to 1, at `frequency` cycles per sample (finite;
    /// negative runs backward) with the peak at `pw`. The phase travels to
    /// `offset` (in `[0, 1)`) as extra frequency, by the shortest way and as
    /// fast as ±`max` cycles per sample allows, so every corner stays
    /// band-limited and an oscillator that moves to an offset plays as one
    /// seeded there does.
    #[inline(always)]
    pub fn step(&mut self, frequency: f32, pw: f32, offset: f32, max: f32) -> f32 {
        self.frequency.set(frequency, Some(1.0));
        self.pw.set(pw, Some(1.0));
        let frequency = self.frequency.next_value();
        let behind = offset - self.offset;
        // The bounds always hold 0, so a phase with no way left to travel never
        // shifts, even when the ramp lands a hair past ±`max`.
        let shift = (behind - behind.round())
            .clamp((-max - frequency).min(0.0), (max - frequency).max(0.0));
        self.offset = wrap(self.offset + shift);
        let frequency = frequency + shift;
        let pw = width(self.pw.next_value(), frequency);

        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        self.phase += frequency;
        let slope_up = 1.0 / pw;
        let slope_down = 1.0 / (1.0 - pw);
        let corner = (slope_up + slope_down) * frequency;

        if self.high ^ (self.phase < pw) {
            let t = edge_time(self.phase - pw, frequency);
            let discontinuity = corner.abs();
            this_sample -= this_integrated_blep_sample(t) * discontinuity;
            next_sample -= next_integrated_blep_sample(t) * discontinuity;
            self.high = self.phase < pw;
        }
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            let t = edge_time(self.phase, frequency);
            this_sample += this_integrated_blep_sample(t) * corner;
            next_sample += next_integrated_blep_sample(t) * corner;
            self.high = true;
        } else if self.phase < 0.0 {
            let t = edge_time(self.phase, frequency);
            self.phase += 1.0;
            this_sample -= this_integrated_blep_sample(t) * corner;
            next_sample -= next_integrated_blep_sample(t) * corner;
            self.high = false;
        }
        next_sample += if self.high {
            self.phase * slope_up
        } else {
            1.0 - (self.phase - pw) * slope_down
        };
        self.next_sample = next_sample;
        2.0 * this_sample - 1.0
    }
}

/// `pw` held far enough from the edges for a sample at `frequency` cycles per
/// sample that each corner stays band-limited.
#[inline(always)]
fn width(pw: f32, frequency: f32) -> f32 {
    // Past 0.25 cycles per sample, reachable at engine rates below 48 kHz,
    // the bounds would pin the peak to an edge and make a slope infinite.
    let speed = frequency.abs().min(0.25);
    constrain(pw, speed * 2.0, 1.0 - 2.0 * speed)
}

/// `plaits::Oscillator` rendering one of its square or saw shapes, at a
/// width of 1/2 and without FM.
#[derive(Clone, Copy)]
pub struct BlepOscillator {
    phase: f32,
    next_sample: f32,
    /// Whether the square is in its high half.
    high: bool,
    /// Plaits' one-sample `ParameterInterpolator`, kept so the oscillator
    /// matches Plaits bit for bit.
    frequency: Ramp,
}

impl Default for BlepOscillator {
    fn default() -> Self {
        let mut frequency = Ramp::default();
        frequency.set(0.001, None);
        Self {
            phase: 0.5,
            next_sample: 0.0,
            high: true,
            frequency,
        }
    }
}

impl BlepOscillator {
    /// Plaits' frequency range for the plain path, in cycles per sample; at
    /// 0.25 the square's width bounds meet at 1/2, so the width never moves.
    const MIN_FREQUENCY: f32 = 0.000001;
    const MAX_FREQUENCY: f32 = 0.25;

    /// The ramped frequency for this sample, advancing the phase by it.
    #[inline(always)]
    fn advance(&mut self, frequency: f32) -> f32 {
        let target = constrain(frequency, Self::MIN_FREQUENCY, Self::MAX_FREQUENCY);
        self.frequency.set(target, Some(1.0));
        let frequency = self.frequency.next_value();
        self.phase += frequency;
        frequency
    }

    /// One sample of the square, from −1 to 1, at `frequency` cycles per
    /// sample.
    #[inline(always)]
    pub fn square(&mut self, frequency: f32) -> f32 {
        let frequency = self.advance(frequency);
        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        if self.high ^ (self.phase >= 0.5) {
            let t = (self.phase - 0.5) / frequency;
            this_sample += this_blep_sample(t);
            next_sample += next_blep_sample(t);
            self.high = self.phase >= 0.5;
        }
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            let t = self.phase / frequency;
            this_sample -= this_blep_sample(t);
            next_sample -= next_blep_sample(t);
            self.high = false;
        }
        next_sample += if self.phase < 0.5 { 0.0 } else { 1.0 };
        self.next_sample = next_sample;
        2.0 * this_sample - 1.0
    }

    /// One sample of the rising saw, from −1 to 1, at `frequency` cycles per
    /// sample.
    #[inline(always)]
    pub fn saw(&mut self, frequency: f32) -> f32 {
        let frequency = self.advance(frequency);
        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            let t = self.phase / frequency;
            this_sample -= this_blep_sample(t);
            next_sample -= next_blep_sample(t);
        }
        next_sample += self.phase;
        self.next_sample = next_sample;
        2.0 * this_sample - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::SlopeOscillator;
    use crate::dsp::mi::core::phase::max_frequency;

    /// With no offset patched the oscillator renders as one with no speed
    /// limit, even when its ramp lands an ulp past a frequency pinned to the
    /// limit.
    #[test]
    fn zero_offset_never_shifts() {
        let max = max_frequency(44100.0);
        let mut limited = SlopeOscillator::default();
        let mut free = SlopeOscillator::default();
        for frequency in [-0.230_158_48, max, max, -max, max] {
            let a = limited.step(frequency, 0.7, 0.0, max);
            let b = free.step(frequency, 0.7, 0.0, f32::INFINITY);
            assert_eq!(a.to_bits(), b.to_bits(), "at {frequency}");
        }
    }

    /// An oscillator seeded at an offset starts on the side of the peak its
    /// first step reads, so it plays as an unseeded one run that far, even
    /// where the width is held in from the requested one.
    #[test]
    fn seed_starts_on_the_constrained_side_of_the_peak() {
        let (frequency, pw) = (0.2, 0.95);
        let mut seeded = SlopeOscillator::default();
        seeded.seed(0.4, frequency, pw);
        let mut plain = SlopeOscillator::default();
        let ahead: Vec<f32> = (0..20)
            .map(|_| seeded.step(frequency, pw, 0.4, 1.0))
            .collect();
        let behind: Vec<f32> = (0..22)
            .map(|_| plain.step(frequency, pw, 0.0, 1.0))
            .collect();
        for n in 1..20 {
            let error = (ahead[n] - behind[n + 2]).abs();
            assert!(error < 1e-4, "sample {n}: error {error}");
        }
    }
}
