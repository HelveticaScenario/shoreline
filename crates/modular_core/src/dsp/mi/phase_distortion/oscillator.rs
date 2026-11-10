//! `plaits::VariableShapeOscillator` as the phase distortion engine drives
//! it: a master phasor that can hard-sync a slave triangle, rendered at twice
//! the sample rate, emitting the phasor offset by the triangle. Extended to
//! run backward: the master wraps below 0, the slave crosses its peak and 0
//! going down, and a backward sync restarts the slave where a forward cycle
//! ends.

use crate::dsp::mi::core::{
    control::Ramp,
    phase::{fraction, wrap_below},
    polyblep::{
        edge_time, next_blep_sample, next_integrated_blep_sample, this_blep_sample,
        this_integrated_blep_sample,
    },
};

/// Substeps per output sample.
const OVERSAMPLING: usize = 2;

/// Slave transitions handled per substep; more than enough for any finite
/// phase, and a bound if one is not.
const MAX_TRANSITIONS: usize = 4;

#[derive(Clone, Copy)]
pub struct PdOscillator {
    master_phase: f32,
    slave_phase: f32,
    next_sample: f32,
    previous_pw: f32,
    /// Whether the slave is past its peak.
    high: bool,
    /// Plaits' `ParameterInterpolator`s across the two substeps.
    master_frequency: Ramp,
    slave_frequency: Ramp,
    pw: Ramp,
    phase_modulation: Ramp,
}

impl Default for PdOscillator {
    fn default() -> Self {
        let ramp = |value| {
            let mut ramp = Ramp::default();
            ramp.set(value, None);
            ramp
        };
        Self {
            master_phase: 0.0,
            slave_phase: 0.0,
            next_sample: 0.0,
            previous_pw: 0.5,
            high: false,
            master_frequency: ramp(0.0),
            slave_frequency: ramp(0.01),
            pw: ramp(0.5),
            phase_modulation: ramp(0.0),
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

/// The slave's naive triangle, with its peak at `pw`, computed as Plaits'
/// `ComputeNaiveSample` does for a pure triangle.
#[inline(always)]
fn triangle(phase: f32, pw: f32, slope_up: f32, slope_down: f32) -> f32 {
    let triangle = if phase < pw {
        phase * slope_up
    } else {
        1.0 - (phase - pw) * slope_down
    };
    phase + (triangle - phase)
}

impl PdOscillator {
    /// The two substeps of one output sample: the phasor offset by
    /// `amount` × the triangle. Frequencies are in cycles per substep (finite,
    /// negative runs backward) and are held to ±`max`.
    #[inline(always)]
    pub fn render(
        &mut self,
        sync: bool,
        master: f32,
        slave: f32,
        pw: f32,
        amount: f32,
        max: f32,
    ) -> [f32; OVERSAMPLING] {
        let master = master.clamp(-max, max);
        let slave = slave.clamp(-max, max);
        let speed = slave.abs();
        let pw = if speed >= 0.25 {
            0.5
        } else {
            constrain(pw, speed * 2.0, 1.0 - 2.0 * speed)
        };
        let length = Some(OVERSAMPLING as f32);
        self.master_frequency.set(master, length);
        self.slave_frequency.set(slave, length);
        self.pw.set(pw, length);
        self.phase_modulation.set(amount, length);

        let mut next_sample = self.next_sample;
        let mut out = [0.0; OVERSAMPLING];
        for out in &mut out {
            let mut this_sample = next_sample;
            next_sample = 0.0;
            let master_frequency = self.master_frequency.next_value();
            let slave_frequency = self.slave_frequency.next_value();
            let pw = self.pw.next_value();
            let slope_up = 1.0 / pw;
            let slope_down = 1.0 / (1.0 - pw);

            let mut reset = false;
            let mut transition_during_reset = false;
            let mut reset_time = 0.0;
            let mut restart = 0.0;
            self.master_phase += master_frequency;
            if self.master_phase >= 1.0 {
                self.master_phase -= 1.0;
                if sync {
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
                    let value = triangle(at_reset, pw, slope_up, slope_down);
                    this_sample -= value * this_blep_sample(reset_time);
                    next_sample -= value * next_blep_sample(reset_time);
                }
            } else if self.master_phase < 0.0 {
                if sync {
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
                    let jump = triangle(restart, pw, slope_up, slope_down)
                        - triangle(at_reset, pw, slope_up, slope_down);
                    this_sample += jump * this_blep_sample(reset_time);
                    next_sample += jump * next_blep_sample(reset_time);
                }
                self.master_phase += 1.0;
            }

            self.slave_phase += slave_frequency;
            if transition_during_reset || !reset {
                let step = (slope_up + slope_down) * slave_frequency.abs();
                let crossing = self.previous_pw - pw + slave_frequency;
                for _ in 0..MAX_TRANSITIONS {
                    if slave_frequency >= 0.0 {
                        if !self.high {
                            if self.slave_phase < pw {
                                break;
                            }
                            let t = edge_time(self.slave_phase - pw, crossing);
                            this_sample -= step * this_integrated_blep_sample(t);
                            next_sample -= step * next_integrated_blep_sample(t);
                            self.high = true;
                        }
                        if self.slave_phase < 1.0 {
                            break;
                        }
                        self.slave_phase -= 1.0;
                        let t = edge_time(self.slave_phase, slave_frequency);
                        this_sample += step * this_integrated_blep_sample(t);
                        next_sample += step * next_integrated_blep_sample(t);
                        self.high = false;
                    } else {
                        if self.high {
                            if self.slave_phase >= pw {
                                break;
                            }
                            let t = edge_time(self.slave_phase - pw, crossing);
                            this_sample -= step * this_integrated_blep_sample(t);
                            next_sample -= step * next_integrated_blep_sample(t);
                            self.high = false;
                        }
                        if self.slave_phase >= 0.0 {
                            break;
                        }
                        let t = edge_time(self.slave_phase, slave_frequency);
                        self.slave_phase += 1.0;
                        this_sample += step * this_integrated_blep_sample(t);
                        next_sample += step * next_integrated_blep_sample(t);
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

            next_sample += triangle(self.slave_phase, pw, slope_up, slope_down);
            self.previous_pw = pw;

            let mut phasor = self.master_phase;
            if sync {
                // A window that fades the triangle out where the phasor wraps,
                // and asymmetry on the phasor itself.
                let w = 4.0 * (1.0 - self.master_phase) * self.master_phase;
                this_sample *= w * (2.0 - w);
                let p2 = phasor * phasor;
                phasor += (p2 * p2 - phasor) * (pw - 0.5).abs() * 2.0;
            }
            *out = phasor + self.phase_modulation.next_value() * this_sample;
        }
        self.next_sample = next_sample;
        out
    }
}
