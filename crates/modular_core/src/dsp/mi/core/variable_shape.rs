//! `plaits::VariableShapeOscillator`: a band-limited waveform that morphs from
//! triangle through saw to a variable-width square, free-running or
//! hard-synced to a master phasor. A negative frequency runs the waveform
//! backward, mirroring each band-limited step; a backward sync restarts the
//! slave where a forward master cycle ends.

use super::control::Ramp;
use super::dsp::constrain;
use super::phase::{BELOW_ONE, fraction, wrap};
use super::polyblep::{
    edge_time, next_blep_sample, next_integrated_blep_sample, this_blep_sample,
    this_integrated_blep_sample, within_sample,
};

/// Transitions handled per sample; more than enough for any finite phase, and
/// a bound if one is not.
const MAX_TRANSITIONS: usize = 4;

#[derive(Clone, Copy)]
pub struct VariableShapeOscillator {
    master_phase: f32,
    phase: f32,
    next_sample: f32,
    previous_pw: f32,
    /// Whether the phase is past `pw`.
    high: bool,
    /// Plaits' one-sample `ParameterInterpolator`s, which land a hair off
    /// their target; kept so the oscillator matches Plaits bit for bit.
    master_frequency: Ramp,
    frequency: Ramp,
    pw: Ramp,
    waveshape: Ramp,
}

impl Default for VariableShapeOscillator {
    fn default() -> Self {
        Self::with_master_phase(0.0)
    }
}

/// The waveform at one width and waveshape; `triangle` and `square` are
/// never both above 0.
#[derive(Clone, Copy)]
struct Shape {
    pw: f32,
    slope_up: f32,
    slope_down: f32,
    triangle: f32,
    square: f32,
}

impl Shape {
    #[inline(always)]
    fn new(pw: f32, waveshape: f32) -> Self {
        Self {
            pw,
            slope_up: 1.0 / pw,
            slope_down: 1.0 / (1.0 - pw),
            triangle: (1.0 - waveshape * 2.0).max(0.0),
            square: (waveshape - 0.5).max(0.0) * 2.0,
        }
    }

    #[inline(always)]
    fn naive(&self, phase: f32) -> f32 {
        let mut saw = phase;
        let square = if phase < self.pw { 0.0 } else { 1.0 };
        let triangle = if phase < self.pw {
            phase * self.slope_up
        } else {
            1.0 - (phase - self.pw) * self.slope_down
        };
        saw += (square - saw) * self.square;
        saw += (triangle - saw) * self.triangle;
        saw
    }
}

/// The width Plaits allows at `speed` cycles per sample.
#[inline(always)]
fn bounded_pw(pw: f32, speed: f32) -> f32 {
    if speed >= 0.25 {
        0.5
    } else {
        constrain(pw, speed * 2.0, 1.0 - 2.0 * speed)
    }
}

impl VariableShapeOscillator {
    /// An oscillator whose master starts `master_phase` into its cycle.
    pub fn with_master_phase(master_phase: f32) -> Self {
        let ramp = |value| {
            let mut ramp = Ramp::default();
            ramp.set(value, None);
            ramp
        };
        Self {
            master_phase,
            phase: 0.0,
            next_sample: 0.0,
            previous_pw: 0.5,
            high: false,
            master_frequency: ramp(0.0),
            frequency: ramp(0.01),
            pw: ramp(0.5),
            waveshape: ramp(0.0),
        }
    }

    /// One free-running sample, from −1 to 1, at `frequency` cycles per
    /// sample (finite and within ±0.25, which the caller ensures), `pw` the
    /// fraction of the cycle before the square's edge or the triangle's peak,
    /// and `waveshape` from triangle (0) through saw (0.5) to square (1).
    #[inline(always)]
    pub fn step(&mut self, frequency: f32, pw: f32, waveshape: f32) -> f32 {
        self.frequency.set(frequency, Some(1.0));
        self.pw.set(bounded_pw(pw, frequency.abs()), Some(1.0));
        self.waveshape.set(waveshape, Some(1.0));
        let frequency = self.frequency.next_value();
        let shape = Shape::new(self.pw.next_value(), self.waveshape.next_value());

        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        let crossing = self.previous_pw - shape.pw + frequency;
        self.phase += frequency;
        self.transitions(
            frequency,
            crossing,
            &shape,
            &mut this_sample,
            &mut next_sample,
        );
        self.finish(&shape, this_sample, next_sample)
    }

    /// One sample, from −1 to 1, of the slave at `slave` cycles per sample
    /// restarted by the master at `master`: both finite and of the same sign
    /// (negative runs backward), held to ±`max`. `pw` and `waveshape` shape
    /// the slave as in [`step`](Self::step).
    #[inline(always)]
    pub fn step_synced(
        &mut self,
        master: f32,
        slave: f32,
        pw: f32,
        waveshape: f32,
        max: f32,
    ) -> f32 {
        let master = master.clamp(-max, max);
        let slave = slave.clamp(-max, max);
        self.master_frequency.set(master, Some(1.0));
        self.frequency.set(slave, Some(1.0));
        self.pw.set(bounded_pw(pw, slave.abs()), Some(1.0));
        self.waveshape.set(waveshape, Some(1.0));

        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        let master_frequency = self.master_frequency.next_value();
        let frequency = self.frequency.next_value();
        let shape = Shape::new(self.pw.next_value(), self.waveshape.next_value());
        let pw = shape.pw;

        let mut reset = false;
        let mut transition_during_reset = false;
        let mut reset_time = 0.0;
        let mut restart = 0.0;
        self.master_phase += master_frequency;
        // Wrapping by the floor keeps up with a frequency past a whole cycle
        // per sample, which engine rates below 12 kHz allow.
        if self.master_phase >= 1.0 {
            self.master_phase = wrap(self.master_phase);
            reset_time = edge_time(self.master_phase, master_frequency);
            let mut at_reset = self.phase + (1.0 - reset_time) * frequency;
            reset = true;
            if at_reset >= 1.0 {
                at_reset -= 1.0;
                transition_during_reset = true;
            }
            if !self.high && at_reset >= pw {
                transition_during_reset = true;
            }
            let value = shape.naive(at_reset);
            this_sample -= value * this_blep_sample(reset_time);
            next_sample -= value * next_blep_sample(reset_time);
        } else if self.master_phase < 0.0 {
            reset_time = edge_time(self.master_phase, master_frequency);
            let mut at_reset = self.phase + (1.0 - reset_time) * frequency;
            reset = true;
            if at_reset < 0.0 {
                at_reset += 1.0;
                transition_during_reset = true;
            }
            if self.high && at_reset < pw {
                transition_during_reset = true;
            }
            // Where the slave ends a forward master cycle, in (0, 1]: a whole
            // number of slave cycles ends at the top of one.
            restart = fraction(frequency / master_frequency);
            if restart <= 0.0 {
                restart = 1.0;
            }
            let jump = shape.naive(restart) - shape.naive(at_reset);
            this_sample += jump * this_blep_sample(reset_time);
            next_sample += jump * next_blep_sample(reset_time);
            self.master_phase = wrap(self.master_phase);
        }

        let crossing = self.previous_pw - pw + frequency;
        self.phase += frequency;
        if transition_during_reset || !reset {
            self.transitions(
                frequency,
                crossing,
                &shape,
                &mut this_sample,
                &mut next_sample,
            );
        }
        if reset {
            // The slave runs on from its restart for the rest of the sample,
            // which can carry it across its width or its end below 12 kHz, or
            // backward from a restart just above its width.
            if master_frequency >= 0.0 {
                self.phase = 0.0;
                self.high = false;
            } else {
                self.phase = restart;
                self.high = restart >= pw;
            }
            self.phase += reset_time * frequency;
            self.transitions(
                frequency,
                crossing,
                &shape,
                &mut this_sample,
                &mut next_sample,
            );
        }
        self.finish(&shape, this_sample, next_sample)
    }

    /// The band-limited steps for each width crossing and wrap the phase made
    /// on its way to where it is, moving at `frequency`.
    #[inline(always)]
    fn transitions(
        &mut self,
        frequency: f32,
        crossing: f32,
        shape: &Shape,
        this_sample: &mut f32,
        next_sample: &mut f32,
    ) {
        let pw = shape.pw;
        let corner = (shape.slope_up + shape.slope_down) * frequency.abs() * shape.triangle;
        let wrap_step = 1.0 - shape.triangle;
        for _ in 0..MAX_TRANSITIONS {
            if frequency >= 0.0 {
                if !self.high {
                    if self.phase < pw {
                        break;
                    }
                    let t = edge_time(self.phase - pw, crossing);
                    if within_sample(t) {
                        *this_sample += shape.square * this_blep_sample(t);
                        *next_sample += shape.square * next_blep_sample(t);
                        *this_sample -= corner * this_integrated_blep_sample(t);
                        *next_sample -= corner * next_integrated_blep_sample(t);
                    }
                    self.high = true;
                }
                if self.phase < 1.0 {
                    break;
                }
                self.phase -= 1.0;
                let t = edge_time(self.phase, frequency);
                *this_sample -= wrap_step * this_blep_sample(t);
                *next_sample -= wrap_step * next_blep_sample(t);
                *this_sample += corner * this_integrated_blep_sample(t);
                *next_sample += corner * next_integrated_blep_sample(t);
                self.high = false;
            } else {
                if self.high {
                    if self.phase >= pw {
                        break;
                    }
                    let t = edge_time(self.phase - pw, crossing);
                    if within_sample(t) {
                        *this_sample -= shape.square * this_blep_sample(t);
                        *next_sample -= shape.square * next_blep_sample(t);
                        *this_sample -= corner * this_integrated_blep_sample(t);
                        *next_sample -= corner * next_integrated_blep_sample(t);
                    }
                    self.high = false;
                }
                if self.phase >= 0.0 {
                    break;
                }
                let t = edge_time(self.phase, frequency);
                // A phase a hair below 0 rounds up to 1 here; the largest
                // value below 1 keeps it within the cycle.
                self.phase = (self.phase + 1.0).min(BELOW_ONE);
                *this_sample += wrap_step * this_blep_sample(t);
                *next_sample += wrap_step * next_blep_sample(t);
                *this_sample += corner * this_integrated_blep_sample(t);
                *next_sample += corner * next_integrated_blep_sample(t);
                self.high = true;
            }
        }
    }

    /// Brings the phase back within its cycle and emits the sample.
    #[inline(always)]
    fn finish(&mut self, shape: &Shape, this_sample: f32, mut next_sample: f32) -> f32 {
        if !(0.0..1.0).contains(&self.phase) {
            self.phase = wrap(self.phase);
            self.high = self.phase >= shape.pw;
        }
        next_sample += shape.naive(self.phase);
        self.previous_pw = shape.pw;
        self.next_sample = next_sample;
        2.0 * this_sample - 1.0
    }
}

#[cfg(test)]
mod tests;
