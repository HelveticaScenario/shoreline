//! Four one-pole stages in a resonant loop: the transistor ladder, whose
//! stages each saturate their own input and output, and the OTA cascade,
//! whose stages saturate the difference between them.
//!
//! Each sample solves the trapezoidal loop exactly for stage gains
//! linearised around the previous sample's voltages, so the feedback path
//! carries no delay and the cutoff stays in tune under fast modulation.

use std::marker::PhantomData;

use super::kernel::{Kernel, LoopCoeffs, LoopResponse, Mix, Rates, Taps, UNUSED};
use crate::dsp::filters::resonant_svf::Response;
use crate::dsp::utils::sanitize;
use crate::dsp::utils::saturation::tanh_ratio;

/// Loop gain at which the four-pole loop self-oscillates.
const SELF_OSCILLATION: f32 = 4.0;

/// How a stage's saturation sets the two gains of its trapezoidal step
/// `v = a·s + b·input`.
pub(crate) trait Stages: Copy + Default {
    /// `(a, b)` for each stage at integrator gain `f`, from the previous
    /// sample's loop input `u` and stage voltages `v`.
    fn gains(f: f32, u: f32, v: &[f32; 4]) -> ([f32; 4], [f32; 4]);
    /// The loop gain after the feedback path's own saturation, given the
    /// previous sample's last stage voltage.
    fn feedback(k: f32, v4: f32) -> f32;
}

/// Each stage is a differential pair: its drive is `tanh(in) − tanh(out)`.
#[derive(Clone, Copy, Default)]
pub struct LadderStages;

impl Stages for LadderStages {
    #[inline]
    fn gains(f: f32, u: f32, v: &[f32; 4]) -> ([f32; 4], [f32; 4]) {
        let t = [
            tanh_ratio(u),
            tanh_ratio(v[0]),
            tanh_ratio(v[1]),
            tanh_ratio(v[2]),
            tanh_ratio(v[3]),
        ];
        let a: [f32; 4] = std::array::from_fn(|i| 1.0 / (1.0 + f * t[i + 1]));
        (a, std::array::from_fn(|i| f * t[i] * a[i]))
    }

    #[inline]
    fn feedback(k: f32, _: f32) -> f32 {
        k
    }
}

/// Level the OTA cascade's feedback saturates at, low enough that the first
/// stage stays out of slewing and self-oscillation stays in tune.
const FEEDBACK_CLIP: f32 = 0.5;

/// Each stage is an OTA integrator: its drive is `tanh(in − out)`. The
/// resonance amplifier is an OTA as well, so the feedback saturates and
/// holds self-oscillation below the level where the stages slew.
#[derive(Clone, Copy, Default)]
pub struct OtaStages;

impl Stages for OtaStages {
    #[inline]
    fn gains(f: f32, u: f32, v: &[f32; 4]) -> ([f32; 4], [f32; 4]) {
        let ft = [
            f * tanh_ratio(u - v[0]),
            f * tanh_ratio(v[0] - v[1]),
            f * tanh_ratio(v[1] - v[2]),
            f * tanh_ratio(v[2] - v[3]),
        ];
        let a: [f32; 4] = std::array::from_fn(|i| 1.0 / (1.0 + ft[i]));
        (a, std::array::from_fn(|i| ft[i] * a[i]))
    }

    #[inline]
    fn feedback(k: f32, v4: f32) -> f32 {
        k * tanh_ratio(k * v4 / FEEDBACK_CLIP)
    }
}

/// The 6, 12, 18 and 24 dB outputs as weights on the loop input and the
/// four stage voltages. With `G = 1 / (s + 1)` one stage's response, stage
/// `n` carries `Gⁿ`, so each row is its response's numerator over
/// `(s + 1)⁴` expanded in powers of `G`. The 24 dB notch's nulls sit half an
/// octave either side of the cutoff.
const fn mixes(response: LoopResponse) -> [Mix; 4] {
    match response {
        LoopResponse::Base(Response::Lowpass) => [
            [0.0, 1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ],
        LoopResponse::Base(Response::Highpass) => [
            [1.0, -1.0, 0.0, 0.0, 0.0],
            [1.0, -2.0, 1.0, 0.0, 0.0],
            [1.0, -3.0, 3.0, -1.0, 0.0],
            [1.0, -4.0, 6.0, -4.0, 1.0],
        ],
        LoopResponse::Base(Response::Bandpass) => [
            UNUSED,
            [0.0, 2.0, -2.0, 0.0, 0.0],
            UNUSED,
            [0.0, 0.0, 4.0, -8.0, 4.0],
        ],
        LoopResponse::Base(Response::Notch) => [
            UNUSED,
            [1.0, -2.0, 2.0, 0.0, 0.0],
            UNUSED,
            [1.0, -4.0, 8.5, -9.0, 4.5],
        ],
        LoopResponse::Allpass => [
            UNUSED,
            [1.0, -4.0, 4.0, 0.0, 0.0],
            UNUSED,
            [1.0, -8.0, 24.0, -32.0, 16.0],
        ],
    }
}

#[derive(Clone, Copy, Default)]
pub struct CascadeState {
    /// Trapezoidal integrator states.
    s: [f32; 4],
    /// The previous sample's stage voltages and loop input.
    v: [f32; 4],
    u: f32,
}

/// A four-stage loop with `S`'s saturation.
#[derive(Clone, Copy, Default)]
pub struct Cascade<S>(PhantomData<S>);

impl<S: Stages> Kernel for Cascade<S> {
    type Response = LoopResponse;
    type Options = ();
    type Coeffs = LoopCoeffs;
    type State = CascadeState;
    const VOLTS: f32 = 8.0;

    fn coeffs(
        response: LoopResponse,
        cutoff: f32,
        resonance: f32,
        rates: Rates,
        _: Taps,
        _: (),
    ) -> LoopCoeffs {
        LoopCoeffs::new(
            rates.gain(cutoff),
            resonance,
            SELF_OSCILLATION,
            mixes(response),
            response,
        )
    }

    #[inline]
    fn step(
        _: LoopResponse,
        c: &LoopCoeffs,
        state: &mut CascadeState,
        x: f32,
        taps: Taps,
    ) -> [f32; 4] {
        let (a, b) = S::gains(c.f, state.u, &state.v);
        let s = &mut state.s;
        // The last stage as `p + q·u`, then the loop `u = x − k·v4`.
        let mut p = a[0] * s[0];
        let mut q = b[0];
        for i in 1..4 {
            p = a[i] * s[i] + b[i] * p;
            q *= b[i];
        }
        let k = S::feedback(c.k, state.v[3]);
        let u = x - k * (p + q * x) / (1.0 + k * q);
        let mut v = [0.0; 4];
        let mut drive = u;
        for i in 0..4 {
            v[i] = a[i] * s[i] + b[i] * drive;
            s[i] = sanitize(2.0 * v[i] - s[i]);
            drive = v[i];
        }
        state.v = v.map(sanitize);
        state.u = sanitize(u);
        c.outputs(x, u, &v, taps)
    }
}
