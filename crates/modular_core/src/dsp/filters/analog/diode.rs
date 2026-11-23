//! Diode ladder: four capacitors joined by diode pairs, so each stage loads
//! its neighbours and the poles spread apart instead of coinciding.
//!
//! With `d1..d3` the voltages across the diode pairs between neighbouring
//! nodes and `d4` the last node's own voltage, the nodes follow
//! `y1' = ω(tanh(u) − tanh(d1))` and `yi' = ω/2·(tanh(d(i−1)) − tanh(di))`.
//! Each sample solves the trapezoidal system exactly for diode gains
//! linearised around the previous sample's voltages.

use std::f32::consts::FRAC_1_SQRT_2;

use super::kernel::{Kernel, LoopCoeffs, LoopResponse, Mix, Rates, Taps, UNUSED};
use crate::dsp::filters::resonant_svf::Response;
use crate::dsp::utils::sanitize;
use crate::dsp::utils::saturation::tanh_ratio;

/// Loop gain at which the ladder self-oscillates.
const SELF_OSCILLATION: f32 = 17.0;

/// The 6, 12, 18 and 24 dB outputs as weights on the loop input and the
/// four node voltages. With the loop open and the stages' corner at 1, the
/// input carries the denominator `s⁴ + 4s³ + 5s² + 2s + ⅛` and node `n` a
/// numerator of degree `4 − n`, so each row is the one mix whose numerator
/// is its response's. A lowpass is its node scaled to unity at DC, and an
/// `n`-pole highpass is `sⁿ` times node `n`'s numerator. The 12 dB bandpass
/// is `s` times the second node's numerator and the 24 dB one `s²`. The
/// notches null around the resonant frequency `1/√2`: the 12 dB one there,
/// over `(s + ½)²`, and the 24 dB one half an octave either side, scaled to
/// sit as far above unity below its nulls as below unity above them.
const fn mixes(response: Response) -> [Mix; 4] {
    match response {
        Response::Lowpass => [
            [0.0, 0.25, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0 / 3.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.5, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ],
        Response::Highpass => [
            [1.0, -1.0, 1.0, 0.0, 0.0],
            [1.0, -2.0, 3.5, -2.0, 0.5],
            [1.0, -3.0, 8.0, -10.0, 7.0],
            [1.0, -4.0, 14.0, -24.0, 21.0],
        ],
        Response::Bandpass => [
            UNUSED,
            [0.0, 2.0, -4.0, 2.0, 0.0],
            UNUSED,
            [0.0, 0.0, 8.0, -32.0, 40.0],
        ],
        Response::Notch => [
            UNUSED,
            [1.0, -3.0, 9.5, -14.0, 11.5],
            UNUSED,
            [
                FRAC_1_SQRT_2,
                -4.0 * FRAC_1_SQRT_2,
                16.5 * FRAC_1_SQRT_2,
                -34.0 * FRAC_1_SQRT_2,
                35.5 * FRAC_1_SQRT_2,
            ],
        ],
    }
}

#[derive(Clone, Copy, Default)]
pub struct DiodeState {
    /// Trapezoidal integrator states.
    s: [f32; 4],
    /// The previous sample's diode voltages and loop input.
    d: [f32; 4],
    u: f32,
}

#[derive(Clone, Copy, Default)]
pub struct Diode;

impl Kernel for Diode {
    type Response = Response;
    type Options = ();
    type Coeffs = LoopCoeffs;
    type State = DiodeState;
    const VOLTS: f32 = 8.0;

    fn coeffs(
        response: Response,
        cutoff: f32,
        resonance: f32,
        rates: Rates,
        _: Taps,
        _: (),
    ) -> LoopCoeffs {
        LoopCoeffs::new(
            // The loop resonates at 1/√2 of the stages' corner.
            std::f32::consts::SQRT_2 * rates.gain(cutoff),
            resonance,
            SELF_OSCILLATION,
            mixes(response),
            LoopResponse::Base(response),
        )
    }

    #[inline]
    fn step(_: Response, c: &LoopCoeffs, state: &mut DiodeState, x: f32, taps: Taps) -> [f32; 4] {
        let (f, h) = (c.f, 0.5 * c.f);
        let t0 = tanh_ratio(state.u);
        let t = state.d.map(tanh_ratio);
        let s = &mut state.s;
        // Bottom up, each node as `p + q·(the node above)`.
        let q4 = 1.0 / (1.0 + h * (t[2] + t[3]));
        let (p4, q4) = (s[3] * q4, h * t[2] * q4);
        let q3 = 1.0 / (1.0 + h * (t[1] + t[2] - t[2] * q4));
        let (p3, q3) = ((s[2] + h * t[2] * p4) * q3, h * t[1] * q3);
        let q2 = 1.0 / (1.0 + h * (t[0] + t[1] - t[1] * q3));
        let (p2, q2) = ((s[1] + h * t[1] * p3) * q2, h * t[0] * q2);
        let q1 = 1.0 / (1.0 + f * t[0] * (1.0 - q2));
        let (p1, q1) = ((s[0] + f * t[0] * p2) * q1, f * t0 * q1);
        // The last node as `p + q·u`, then the loop `u = x − k·y4`.
        let p = p4 + q4 * (p3 + q3 * (p2 + q2 * p1));
        let q = q4 * q3 * q2 * q1;
        let u = x - c.k * (p + q * x) / (1.0 + c.k * q);
        let y1 = p1 + q1 * u;
        let y2 = p2 + q2 * y1;
        let y3 = p3 + q3 * y2;
        let y4 = p4 + q4 * y3;
        let y = [y1, y2, y3, y4];
        for i in 0..4 {
            s[i] = sanitize(2.0 * y[i] - s[i]);
        }
        state.d = [y1 - y2, y2 - y3, y3 - y4, y4].map(sanitize);
        state.u = sanitize(u);
        c.outputs(x, u, &y, taps)
    }
}
