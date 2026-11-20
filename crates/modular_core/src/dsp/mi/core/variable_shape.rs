//! `plaits::VariableShapeOscillator` without sync: a band-limited waveform that
//! morphs from triangle through saw to a variable-width square. A negative
//! frequency runs the waveform backward, mirroring each band-limited step.

use super::control::Ramp;
use super::polyblep::{
    edge_time, next_blep_sample, next_integrated_blep_sample, this_blep_sample,
    this_integrated_blep_sample,
};

#[derive(Clone, Copy)]
pub struct VariableShapeOscillator {
    phase: f32,
    next_sample: f32,
    previous_pw: f32,
    /// Whether the phase is past `pw`.
    high: bool,
    /// Plaits' one-sample `ParameterInterpolator`s, which land a hair off
    /// their target; kept so the oscillator matches Plaits bit for bit.
    frequency: Ramp,
    pw: Ramp,
    waveshape: Ramp,
}

impl Default for VariableShapeOscillator {
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

impl VariableShapeOscillator {
    /// One sample, from −1 to 1, at `frequency` cycles per sample (finite and
    /// within ±`max`, at most 0.25), `pw` the fraction of the cycle before
    /// the square's edge or the triangle's peak, and `waveshape` from
    /// triangle (0) through saw (0.5) to square (1).
    #[inline(always)]
    pub fn step(&mut self, frequency: f32, pw: f32, waveshape: f32, max: f32) -> f32 {
        let target = frequency.clamp(-max, max);
        let speed = target.abs();
        let pw = if speed >= 0.25 {
            0.5
        } else {
            constrain(pw, speed * 2.0, 1.0 - 2.0 * speed)
        };
        self.frequency.set(target, Some(1.0));
        self.pw.set(pw, Some(1.0));
        self.waveshape.set(waveshape, Some(1.0));
        let frequency = self.frequency.next_value();
        let pw = self.pw.next_value();
        let waveshape = self.waveshape.next_value();

        let square_amount = (waveshape - 0.5).max(0.0) * 2.0;
        let triangle_amount = (1.0 - waveshape * 2.0).max(0.0);
        let slope_up = 1.0 / pw;
        let slope_down = 1.0 / (1.0 - pw);
        let triangle_step = (slope_up + slope_down) * frequency * triangle_amount;

        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        self.phase += frequency;
        // At most one edge and one wrap fall within a sample at these speeds;
        // the bound only guards a `pw` jumping across the phase.
        for _ in 0..3 {
            if frequency >= 0.0 {
                if !self.high {
                    if self.phase < pw {
                        break;
                    }
                    let t = edge_time(self.phase - pw, self.previous_pw - pw + frequency);
                    this_sample += square_amount * this_blep_sample(t);
                    next_sample += square_amount * next_blep_sample(t);
                    this_sample -= triangle_step * this_integrated_blep_sample(t);
                    next_sample -= triangle_step * next_integrated_blep_sample(t);
                    self.high = true;
                }
                if self.phase < 1.0 {
                    break;
                }
                self.phase -= 1.0;
                let t = edge_time(self.phase, frequency);
                this_sample -= (1.0 - triangle_amount) * this_blep_sample(t);
                next_sample -= (1.0 - triangle_amount) * next_blep_sample(t);
                this_sample += triangle_step * this_integrated_blep_sample(t);
                next_sample += triangle_step * next_integrated_blep_sample(t);
                self.high = false;
            } else {
                if self.high {
                    if self.phase >= pw {
                        break;
                    }
                    let t = edge_time(self.phase - pw, self.previous_pw - pw + frequency);
                    this_sample -= square_amount * this_blep_sample(t);
                    next_sample -= square_amount * next_blep_sample(t);
                    this_sample += triangle_step * this_integrated_blep_sample(t);
                    next_sample += triangle_step * next_integrated_blep_sample(t);
                    self.high = false;
                }
                if self.phase >= 0.0 {
                    break;
                }
                let t = edge_time(self.phase, frequency);
                self.phase += 1.0;
                this_sample += (1.0 - triangle_amount) * this_blep_sample(t);
                next_sample += (1.0 - triangle_amount) * next_blep_sample(t);
                this_sample -= triangle_step * this_integrated_blep_sample(t);
                next_sample -= triangle_step * next_integrated_blep_sample(t);
                self.high = true;
            }
        }
        if !(0.0..1.0).contains(&self.phase) {
            self.phase -= self.phase.floor();
            self.high = self.phase >= pw;
        }

        next_sample += naive_sample(
            self.phase,
            pw,
            slope_up,
            slope_down,
            triangle_amount,
            square_amount,
        );
        self.previous_pw = pw;
        self.next_sample = next_sample;
        2.0 * this_sample - 1.0
    }
}

#[inline(always)]
fn naive_sample(
    phase: f32,
    pw: f32,
    slope_up: f32,
    slope_down: f32,
    triangle_amount: f32,
    square_amount: f32,
) -> f32 {
    let mut saw = phase;
    let square = if phase < pw { 0.0 } else { 1.0 };
    let triangle = if phase < pw {
        phase * slope_up
    } else {
        1.0 - (phase - pw) * slope_down
    };
    saw += (square - saw) * square_amount;
    saw += (triangle - saw) * triangle_amount;
    saw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::mi::core::fixture::harmonics;

    /// A frequency running backward plays the forward waveform reversed in
    /// time, so its harmonics match the forward ones, at every shape and
    /// width. Each tone is measured over whole cycles, where the waveform's
    /// DC leaks into no harmonic.
    #[test]
    fn runs_backward() {
        let sample_rate = 48000.0;
        for (hz, pw, waveshape) in [
            (110.0, 0.5, 0.0),
            (220.0, 0.7, 0.25),
            (110.0, 0.5, 0.5),
            (55.0, 0.8, 0.75),
            (440.0, 0.3, 1.0),
        ] {
            let tone = |f: f32| {
                let mut oscillator = VariableShapeOscillator::default();
                let out: Vec<f32> = (0..96000)
                    .map(|_| oscillator.step(f, pw, waveshape, 0.25))
                    .collect();
                harmonics(&out, hz, sample_rate, 16)
            };
            let forward = tone(hz / sample_rate);
            let backward = tone(-hz / sample_rate);
            let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
            for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
                assert!(
                    (b - f).abs() < 0.01 * loudest,
                    "{hz} Hz, pw {pw}, shape {waveshape}, harmonic {}: {b} vs {f}",
                    h + 1
                );
            }
        }
    }

    /// Turning around mid-cycle, at a standstill or at the frequency limit,
    /// keeps the waveform within its swing.
    #[test]
    fn turns_around_cleanly() {
        let mut oscillator = VariableShapeOscillator::default();
        for n in 0..20000 {
            let f = match n % 997 {
                0..=9 => 0.0,
                10..=19 => 0.25,
                20..=29 => -0.25,
                _ => 0.01 * (n as f32 * 0.002).sin(),
            };
            let shape = (n % 5) as f32 * 0.25;
            let out = oscillator.step(f, 0.5 + 0.4 * (n as f32 * 0.01).sin(), shape, 0.25);
            assert!(out.is_finite() && out.abs() < 2.0, "sample {n}: {out}");
        }
    }
}
