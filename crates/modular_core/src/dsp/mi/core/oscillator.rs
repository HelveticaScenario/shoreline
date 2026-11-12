//! `plaits::Oscillator`: the slope shape on its through-zero path, a
//! band-limited triangle whose peak sits at `pw` of the cycle, and the square
//! and saw shapes on its plain path.

use super::control::Ramp;
use super::polyblep::{
    edge_time, next_blep_sample, next_integrated_blep_sample, this_blep_sample,
    this_integrated_blep_sample,
};

#[derive(Clone, Copy)]
pub struct SlopeOscillator {
    phase: f32,
    next_sample: f32,
    /// Whether the phase is past the peak.
    high: bool,
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
            frequency,
            pw,
        }
    }
}

/// stmlib's `CONSTRAIN`: the low bound wins when the bounds cross, which a
/// frequency a hair past 0.25 does.
#[inline(always)]
fn constrain(x: f32, low: f32, high: f32) -> f32 {
    if x < low {
        low
    } else if x > high {
        high
    } else {
        x
    }
}

impl SlopeOscillator {
    /// One sample, from −1 to 1, at `frequency` cycles per sample (finite;
    /// negative runs backward) with the peak at `pw`.
    #[inline(always)]
    pub fn step(&mut self, frequency: f32, pw: f32) -> f32 {
        self.frequency.set(frequency, Some(1.0));
        self.pw.set(pw, Some(1.0));
        let frequency = self.frequency.next_value();
        // Past 0.25 cycles per sample, reachable at engine rates below 48 kHz,
        // the bounds would pin the peak to an edge and make a slope infinite.
        let speed = frequency.abs().min(0.25);
        let pw = constrain(self.pw.next_value(), speed * 2.0, 1.0 - 2.0 * speed);

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
