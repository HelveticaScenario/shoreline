//! The North Coast Synthesis MSK 007 Leapfrog: five integrators in a row,
//! each integrating the difference between its two neighbours, whose
//! outputs a fixed mixer sums into a near-elliptic fifth-order lowpass.
//! Other weights on the mixer, which can also take the core's input, put
//! other zeros over the same five poles: a notch and an allpass.
//!
//! The filter's own response has no resonance. A VCA feeds the mixer's
//! output back, inverted, to the input through a Zener clipper, and the loop
//! self-oscillates where the response turns the phase 180° at a level the
//! feedback gain can make up. Each sample solves the trapezoidal integrators and that loop
//! together, with the clipper's gain linearised around the previous
//! sample's output, so the feedback path carries no delay.

use super::kernel::{DB24, Kernel, Rates, Taps, loop_gain};
use crate::dsp::utils::sanitize;

/// Each integrator's time constant, in seconds with the lowpass's cutoff,
/// the last peak of its passband, at 1 rad/s.
const TAU: [f32; 5] = [
    1.301_466_6,
    1.510_573_6,
    2.319_052_3,
    1.267_479_2,
    1.398_601_4,
];

/// The feedback gain at which the lowpass self-oscillates: the inverse of
/// its level where its phase reaches 180°, at 0.73 of the cutoff.
const LOWPASS_SELF_OSCILLATION: f32 = 1.0 / 0.956_850_6;

/// What the output mixer makes of the core.
#[derive(Clone, Copy)]
pub(crate) enum LeapfrogResponse {
    /// Nulls at 2 and 3 times the cutoff.
    Lowpass,
    /// The zeros of both resonant pole pairs moved onto the frequency
    /// axis: two nulls, at the pairs' own frequencies.
    Notch,
    /// The zeros of both resonant pole pairs mirrored across the frequency
    /// axis, and the real pole cancelled: 720° of phase shift.
    Allpass,
}

struct Mixer {
    /// Weights on the core's input and the five integrators.
    weights: [f32; 6],
    /// The frequency, as a multiple of the lowpass's cutoff, that the
    /// module's frequency param sets: the lowpass's cutoff, the geometric
    /// centre of the notch's nulls, and where the allpass has turned the
    /// phase 360°.
    tuning: f32,
    /// The feedback gain at a resonance of 4.5.
    self_oscillation: f32,
    /// How much of the level the feedback takes away from the passband
    /// comes back.
    makeup: f32,
}

impl LeapfrogResponse {
    const fn mixer(self) -> Mixer {
        match self {
            Self::Lowpass => Mixer {
                weights: [
                    0.0,
                    0.004_474_30,
                    -0.004_831_24,
                    0.193_072_99,
                    -0.171_015_98,
                    0.978_299_9,
                ],
                tuning: 1.0,
                self_oscillation: LOWPASS_SELF_OSCILLATION,
                makeup: 0.5,
            },
            // A notch's phase never reaches 180°, so it takes the lowpass's
            // feedback gain and only sharpens.
            Self::Notch => Mixer {
                weights: [
                    1.0,
                    -0.619_496,
                    0.361_423,
                    -0.034_782_913,
                    -0.295_734_76,
                    0.588_590_6,
                ],
                tuning: 0.848_241_9,
                self_oscillation: LOWPASS_SELF_OSCILLATION,
                makeup: 1.0,
            },
            Self::Allpass => Mixer {
                weights: [
                    1.0,
                    -1.238_992,
                    0.890_873,
                    -0.255_045,
                    -0.870_015_9,
                    1.473_179_8,
                ],
                tuning: 0.952_996_4,
                self_oscillation: 1.0,
                makeup: 1.0,
            },
        }
    }
}

/// Volts at which the input buffer's, an integrator's or the mixer's op amp
/// reaches its supply.
const RAIL: f32 = 10.5;

/// The feedback clipper: above `SOFT_CLIP` volts a 4.7 V Zener pair loads
/// the VCA's output through 39 kΩ beside the 100 kΩ it drives, and a 6.0 V
/// pair holds it at `HARD_CLIP`.
const SOFT_CLIP: f32 = 5.3;
const HARD_CLIP: f32 = 6.6;
const SOFT_SLOPE: f32 = 39.0 / 139.0;

/// The clipper's output over its input `w`, in volts.
#[inline]
fn clip_ratio(w: f32) -> f32 {
    let level = w.abs();
    if level <= SOFT_CLIP {
        1.0
    } else {
        (SOFT_CLIP + SOFT_SLOPE * (level - SOFT_CLIP)).min(HARD_CLIP) / level
    }
}

#[derive(Clone, Copy, Default)]
pub struct LeapfrogCoeffs {
    /// Each integrator's gain.
    g: [f32; 5],
    /// The tridiagonal solve's pivots and upper factors.
    inv: [f32; 5],
    upper: [f32; 4],
    /// The mixer's weights on the core's input and the integrators.
    weights: [f32; 6],
    /// Each integrator's voltage, and the mixer's, per volt at the core's
    /// input.
    q: [f32; 5],
    yq: f32,
    /// Feedback gain, and the output gain that makes up the passband level
    /// the feedback takes away.
    k: f32,
    makeup: f32,
}

impl LeapfrogCoeffs {
    /// The integrator voltages for the right-hand side `rhs`.
    #[inline]
    fn solve(&self, rhs: [f32; 5]) -> [f32; 5] {
        let mut v = rhs;
        v[0] *= self.inv[0];
        for i in 1..5 {
            v[i] = (v[i] + self.g[i] * v[i - 1]) * self.inv[i];
        }
        for i in (0..4).rev() {
            v[i] -= self.upper[i] * v[i + 1];
        }
        v
    }

    /// The mixer's output for the integrator voltages `v`, less its share
    /// of the core's input.
    #[inline]
    fn mix(&self, v: &[f32; 5]) -> f32 {
        self.weights[1..].iter().zip(v).map(|(a, v)| a * v).sum()
    }
}

#[derive(Clone, Copy, Default)]
pub struct LeapfrogState {
    /// Trapezoidal integrator states.
    s: [f32; 5],
    /// The previous sample's mixer output.
    y: f32,
}

/// The five-integrator leapfrog core and its feedback loop, in volts.
#[derive(Clone, Copy, Default)]
pub struct Leapfrog;

impl Kernel for Leapfrog {
    type Response = LeapfrogResponse;
    type Options = ();
    type Coeffs = LeapfrogCoeffs;
    type State = LeapfrogState;
    const VOLTS: f32 = 1.0;

    fn coeffs(
        response: LeapfrogResponse,
        cutoff: f32,
        resonance: f32,
        rates: Rates,
        _: Taps,
        _: (),
    ) -> LeapfrogCoeffs {
        let mixer = response.mixer();
        let gain = rates.gain(cutoff) / mixer.tuning;
        let g = TAU.map(|tau| gain / tau);
        // Integrator `i` solves `v[i] = s[i] + g[i]·(v[i−1] − v[i+1])`; the
        // first reads the core's input for `v[i−1]` and the last its own
        // output for `v[i+1]`.
        let mut inv = [1.0; 5];
        let mut upper = [0.0; 4];
        for i in 0..5 {
            let diagonal = if i == 4 { 1.0 + g[4] } else { 1.0 };
            let below = if i == 0 { 0.0 } else { g[i] * upper[i - 1] };
            inv[i] = 1.0 / (diagonal + below);
            if i < 4 {
                upper[i] = g[i] * inv[i];
            }
        }
        let mut c = LeapfrogCoeffs {
            g,
            inv,
            upper,
            weights: mixer.weights,
            ..Default::default()
        };
        c.q = c.solve([g[0], 0.0, 0.0, 0.0, 0.0]);
        c.yq = c.weights[0] + c.mix(&c.q);
        c.k = loop_gain(resonance, mixer.self_oscillation);
        c.makeup = 1.0 + mixer.makeup * c.k;
        c
    }

    #[inline]
    fn step(
        _: LeapfrogResponse,
        c: &LeapfrogCoeffs,
        state: &mut LeapfrogState,
        x: f32,
        taps: Taps,
    ) -> [f32; 4] {
        let mut v = c.solve(state.s);
        // The core's input `u = x − k·y`, with `y` the mix of `u` and
        // `v + q·u`.
        let k = c.k * clip_ratio(c.k * state.y);
        let u = ((x - k * c.mix(&v)) / (1.0 + k * c.yq)).clamp(-RAIL, RAIL);
        for i in 0..5 {
            let ideal = v[i] + c.q[i] * u;
            v[i] = ideal.clamp(-RAIL, RAIL);
            // An integrator at its rail keeps integrating from there.
            state.s[i] = sanitize(v[i] + ideal - state.s[i]);
        }
        let y = sanitize((c.weights[0] * u + c.mix(&v)).clamp(-RAIL, RAIL));
        state.y = y;
        let mut out = [0.0; 4];
        if taps[DB24] {
            out[DB24] = c.makeup * y;
        }
        out
    }
}
