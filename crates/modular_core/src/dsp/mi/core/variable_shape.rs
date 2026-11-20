//! `plaits::VariableShapeOscillator` on its hard-sync path: a master phasor
//! restarting a slave whose waveform morphs from triangle through saw to a
//! square of variable width. Extended to run backward: the master wraps below
//! 0, the slave crosses its width and 0 going down, and a backward sync
//! restarts the slave where a forward cycle ends.

use super::control::Ramp;
use super::phase::{fraction, wrap_below};
use super::polyblep::{
    edge_time, next_blep_sample, next_integrated_blep_sample, this_blep_sample,
    this_integrated_blep_sample,
};

/// Slave transitions handled per sample; more than enough for any finite
/// phase, and a bound if one is not.
const MAX_TRANSITIONS: usize = 4;

#[derive(Clone, Copy)]
pub struct VariableShapeOscillator {
    master_phase: f32,
    slave_phase: f32,
    next_sample: f32,
    previous_pw: f32,
    /// Whether the slave is past its width.
    high: bool,
    /// Plaits' one-sample `ParameterInterpolator`s, which land a hair off
    /// their target; kept so the oscillator matches Plaits bit for bit.
    master_frequency: Ramp,
    slave_frequency: Ramp,
    pw: Ramp,
    waveshape: Ramp,
}

impl Default for VariableShapeOscillator {
    fn default() -> Self {
        Self::with_master_phase(0.0)
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

/// The waveform's mix at one waveshape: `triangle` and `square` are never both
/// above 0.
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
            slave_phase: 0.0,
            next_sample: 0.0,
            previous_pw: 0.5,
            high: false,
            master_frequency: ramp(0.0),
            slave_frequency: ramp(0.01),
            pw: ramp(0.5),
            waveshape: ramp(0.0),
        }
    }

    /// One sample, from −1 to 1, of the slave at `slave` cycles per sample
    /// restarted by the master at `master` (both finite, negative runs
    /// backward, held to ±`max`), the slave's width at `pw` and its waveshape
    /// from triangle (0) through saw (1/2) to square (1).
    #[inline(always)]
    pub fn step(&mut self, master: f32, slave: f32, pw: f32, waveshape: f32, max: f32) -> f32 {
        let master = master.clamp(-max, max);
        let slave = slave.clamp(-max, max);
        let speed = slave.abs();
        let pw = if speed >= 0.25 {
            0.5
        } else {
            constrain(pw, speed * 2.0, 1.0 - 2.0 * speed)
        };
        self.master_frequency.set(master, Some(1.0));
        self.slave_frequency.set(slave, Some(1.0));
        self.pw.set(pw, Some(1.0));
        self.waveshape.set(waveshape, Some(1.0));

        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        let master_frequency = self.master_frequency.next_value();
        let slave_frequency = self.slave_frequency.next_value();
        let pw = self.pw.next_value();
        let waveshape = self.waveshape.next_value();
        let shape = Shape {
            pw,
            slope_up: 1.0 / pw,
            slope_down: 1.0 / (1.0 - pw),
            triangle: (1.0 - waveshape * 2.0).max(0.0),
            square: (waveshape - 0.5).max(0.0) * 2.0,
        };

        let mut reset = false;
        let mut transition_during_reset = false;
        let mut reset_time = 0.0;
        let mut restart = 0.0;
        self.master_phase += master_frequency;
        // Wrapping by the floor keeps up with a frequency past a whole cycle
        // per sample, which engine rates below 12 kHz allow.
        if self.master_phase >= 1.0 {
            self.master_phase -= self.master_phase.floor();
            reset_time = edge_time(self.master_phase, master_frequency);
            let mut at_reset = self.slave_phase + (1.0 - reset_time) * slave_frequency;
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
            let mut at_reset = self.slave_phase + (1.0 - reset_time) * slave_frequency;
            reset = true;
            if at_reset < 0.0 {
                at_reset += 1.0;
                transition_during_reset = true;
            }
            if self.high && at_reset < pw {
                transition_during_reset = true;
            }
            restart = fraction(slave_frequency / master_frequency);
            let jump = shape.naive(restart) - shape.naive(at_reset);
            this_sample += jump * this_blep_sample(reset_time);
            next_sample += jump * next_blep_sample(reset_time);
            self.master_phase -= self.master_phase.floor();
        }

        self.slave_phase += slave_frequency;
        if transition_during_reset || !reset {
            let corner =
                (shape.slope_up + shape.slope_down) * slave_frequency.abs() * shape.triangle;
            let wrap = 1.0 - shape.triangle;
            let crossing = self.previous_pw - pw + slave_frequency;
            for _ in 0..MAX_TRANSITIONS {
                if slave_frequency >= 0.0 {
                    if !self.high {
                        if self.slave_phase < pw {
                            break;
                        }
                        let t = edge_time(self.slave_phase - pw, crossing);
                        this_sample += shape.square * this_blep_sample(t);
                        next_sample += shape.square * next_blep_sample(t);
                        this_sample -= corner * this_integrated_blep_sample(t);
                        next_sample -= corner * next_integrated_blep_sample(t);
                        self.high = true;
                    }
                    if self.slave_phase < 1.0 {
                        break;
                    }
                    self.slave_phase -= 1.0;
                    let t = edge_time(self.slave_phase, slave_frequency);
                    this_sample -= wrap * this_blep_sample(t);
                    next_sample -= wrap * next_blep_sample(t);
                    this_sample += corner * this_integrated_blep_sample(t);
                    next_sample += corner * next_integrated_blep_sample(t);
                    self.high = false;
                } else {
                    if self.high {
                        if self.slave_phase >= pw {
                            break;
                        }
                        let t = edge_time(self.slave_phase - pw, crossing);
                        this_sample -= shape.square * this_blep_sample(t);
                        next_sample -= shape.square * next_blep_sample(t);
                        this_sample -= corner * this_integrated_blep_sample(t);
                        next_sample -= corner * next_integrated_blep_sample(t);
                        self.high = false;
                    }
                    if self.slave_phase >= 0.0 {
                        break;
                    }
                    let t = edge_time(self.slave_phase, slave_frequency);
                    self.slave_phase += 1.0;
                    this_sample += wrap * this_blep_sample(t);
                    next_sample += wrap * next_blep_sample(t);
                    this_sample += corner * this_integrated_blep_sample(t);
                    next_sample += corner * next_integrated_blep_sample(t);
                    self.high = true;
                }
            }
        }

        if reset {
            if master_frequency >= 0.0 {
                self.slave_phase = reset_time * slave_frequency;
                self.high = false;
            } else {
                self.slave_phase = wrap_below(restart + reset_time * slave_frequency);
                self.high = self.slave_phase >= pw;
            }
        }

        next_sample += shape.naive(self.slave_phase);
        self.previous_pw = pw;
        self.next_sample = next_sample;
        2.0 * this_sample - 1.0
    }
}
