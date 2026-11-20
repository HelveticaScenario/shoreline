//! `plaits::VariableSawOscillator`: a saw that holds still past its width,
//! leaving a notch, morphing into a triangle peaking at that width. Extended
//! to run backward, crossing its width and 0 going down.

use super::control::Ramp;
use super::polyblep::{
    edge_time, next_blep_sample, next_integrated_blep_sample, this_blep_sample,
    this_integrated_blep_sample,
};

/// How far past the saw's peak the notch holds, as a fraction of its height.
const NOTCH_DEPTH: f32 = 0.2;

/// Transitions handled per sample; more than enough for any finite phase, and
/// a bound if one is not.
const MAX_TRANSITIONS: usize = 4;

#[derive(Clone, Copy)]
pub struct VariableSawOscillator {
    phase: f32,
    next_sample: f32,
    previous_pw: f32,
    /// Whether the phase is past the width.
    high: bool,
    /// Plaits' one-sample `ParameterInterpolator`s, which land a hair off
    /// their target; kept so the oscillator matches Plaits bit for bit.
    frequency: Ramp,
    pw: Ramp,
    waveshape: Ramp,
}

impl Default for VariableSawOscillator {
    fn default() -> Self {
        let ramp = |value| {
            let mut ramp = Ramp::default();
            ramp.set(value, None);
            ramp
        };
        Self {
            phase: 0.0,
            next_sample: 0.0,
            previous_pw: 0.5,
            high: false,
            frequency: ramp(0.01),
            pw: ramp(0.5),
            waveshape: ramp(0.0),
        }
    }
}

/// stmlib's `CONSTRAIN`: the low bound wins when the bounds cross.
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

impl VariableSawOscillator {
    /// One sample, from about −1 to 1, at `frequency` cycles per sample
    /// (finite, negative runs backward, held to ±`max`), the width at `pw`
    /// and the waveshape from notched saw (0) to triangle (1).
    #[inline(always)]
    pub fn step(&mut self, frequency: f32, pw: f32, waveshape: f32, max: f32) -> f32 {
        let frequency = frequency.clamp(-max, max);
        let speed = frequency.abs();
        let pw = if speed >= 0.25 {
            0.5
        } else {
            constrain(pw, speed * 2.0, 1.0 - 2.0 * speed)
        };
        self.frequency.set(frequency, Some(1.0));
        self.pw.set(pw, Some(1.0));
        self.waveshape.set(waveshape, Some(1.0));

        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        let frequency = self.frequency.next_value();
        let pw = self.pw.next_value();
        let triangle_amount = self.waveshape.next_value();
        let notch_amount = 1.0 - triangle_amount;
        let slope_up = 1.0 / pw;
        let slope_down = 1.0 / (1.0 - pw);
        let corner = (slope_up + slope_down) * frequency.abs() * triangle_amount;
        let notch = (NOTCH_DEPTH + 1.0 - pw) * notch_amount;
        let wrap = (NOTCH_DEPTH + 1.0) * notch_amount;
        let crossing = self.previous_pw - pw + frequency;

        self.phase += frequency;
        for _ in 0..MAX_TRANSITIONS {
            if frequency >= 0.0 {
                if !self.high && self.phase >= pw {
                    let t = edge_time(self.phase - pw, crossing);
                    this_sample += notch * this_blep_sample(t);
                    next_sample += notch * next_blep_sample(t);
                    this_sample -= corner * this_integrated_blep_sample(t);
                    next_sample -= corner * next_integrated_blep_sample(t);
                    self.high = true;
                } else if self.phase >= 1.0 {
                    self.phase -= 1.0;
                    let t = edge_time(self.phase, frequency);
                    this_sample -= wrap * this_blep_sample(t);
                    next_sample -= wrap * next_blep_sample(t);
                    this_sample += corner * this_integrated_blep_sample(t);
                    next_sample += corner * next_integrated_blep_sample(t);
                    self.high = false;
                } else {
                    break;
                }
            } else if self.high && self.phase < pw {
                let t = edge_time(self.phase - pw, crossing);
                this_sample -= notch * this_blep_sample(t);
                next_sample -= notch * next_blep_sample(t);
                this_sample -= corner * this_integrated_blep_sample(t);
                next_sample -= corner * next_integrated_blep_sample(t);
                self.high = false;
            } else if self.phase < 0.0 {
                let t = edge_time(self.phase, frequency);
                self.phase += 1.0;
                this_sample += wrap * this_blep_sample(t);
                next_sample += wrap * next_blep_sample(t);
                this_sample += corner * this_integrated_blep_sample(t);
                next_sample += corner * next_integrated_blep_sample(t);
                self.high = true;
            } else {
                break;
            }
        }

        let notch_saw = if self.phase < pw {
            self.phase
        } else {
            1.0 + NOTCH_DEPTH
        };
        let triangle = if self.phase < pw {
            self.phase * slope_up
        } else {
            1.0 - (self.phase - pw) * slope_down
        };
        next_sample += notch_saw * notch_amount + triangle * triangle_amount;
        self.previous_pw = pw;
        self.next_sample = next_sample;
        (2.0 * this_sample - 1.0) / (1.0 + NOTCH_DEPTH)
    }
}
