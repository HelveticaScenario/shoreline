//! Sallen-Key: two one-pole lowpasses in series with positive feedback of
//! their difference through a clipper, `u = x + tanh(k·(y1 − y2))`. Only the
//! feedback clips, so a loud input passes at full level and swamps the
//! resonance rather than being limited with it. The
//! 12 dB output is one such stage; the 24 dB output is its own pair of
//! them: Butterworth-flat at resonance 0 for a lowpass or highpass, a
//! Butterworth bandpass pair whose width follows the resonance, or for a
//! notch, half an octave either side of the centre.
//!
//! Each stage solves its delay-free loop for the difference with Newton's
//! method. The loop's slope stays above 0.4 over the whole resonance range,
//! so the equation has one root, which two steps from the previous sample's
//! value close in on.

use super::kernel::{DB12, DB24, Kernel, RESONANCE_AT_SELF_OSCILLATION, Rates, Taps, loop_gain};
use crate::dsp::filters::resonant_svf::{
    BUTTERWORTH_K_A, BUTTERWORTH_Q_B, NOTCH_SPREAD_OCTAVES, Response, bandpass_pair,
};
use crate::dsp::utils::sanitize;
use crate::dsp::utils::saturation::tanh_and_slope;

/// Feedback gain at which a stage self-oscillates; its Q is `1 / (2 − k)`.
const SELF_OSCILLATION: f32 = 2.0;
/// Feedback gains of the two sections that are Butterworth-flat together.
const BUTTERWORTH_FEEDBACK_A: f32 = SELF_OSCILLATION - BUTTERWORTH_K_A;
const BUTTERWORTH_FEEDBACK_B: f32 = SELF_OSCILLATION - 1.0 / BUTTERWORTH_Q_B;
/// Feedback gain from which a stage rings on for longer than a listener
/// would miss: a Q of 10 and up, through self-oscillation.
const RINGS_ON: f32 = SELF_OSCILLATION - 0.1;

/// Highest Q the bandpass pair is tuned for; feedback past self-oscillation
/// adds to both sections.
const MAX_PAIR_Q: f32 = 50.0;

#[derive(Clone, Copy, Default)]
struct StageCoeffs {
    /// One-pole gain `g / (1 + g)`.
    gain: f32,
    k: f32,
}

impl StageCoeffs {
    fn new(g: f32, k: f32) -> Self {
        Self {
            gain: g / (1.0 + g),
            k,
        }
    }

    /// The stage's drive `u` for a difference `d`, and its slope in `d`.
    #[inline]
    fn drive(&self, x: f32, d: f32) -> (f32, f32) {
        let (feedback, slope) = tanh_and_slope(self.k * d);
        (x + feedback, self.k * slope)
    }
}

#[derive(Clone, Copy, Default)]
struct Stage {
    s1: f32,
    s2: f32,
    /// The previous sample's `y1 − y2`.
    d: f32,
}

impl Stage {
    #[inline]
    fn step(&mut self, c: &StageCoeffs, response: Response, x: f32) -> f32 {
        let rest = 1.0 - c.gain;
        let (hold1, hold2) = (rest * self.s1, rest * self.s2);
        // `d = a·u(d) + b`.
        let a = c.gain * rest;
        let b = rest * hold1 - hold2;
        let mut d = self.d;
        for _ in 0..2 {
            let (u, slope) = c.drive(x, d);
            d -= (d - a * u - b) / (1.0 - a * slope);
        }
        let u = c.drive(x, d).0;
        let y1 = c.gain * u + hold1;
        let y2 = c.gain * y1 + hold2;
        self.s1 = sanitize(2.0 * y1 - self.s1);
        self.s2 = sanitize(2.0 * y2 - self.s2);
        self.d = sanitize(y1 - y2);
        match response {
            Response::Lowpass => y2,
            Response::Highpass => u - 2.0 * y1 + y2,
            Response::Bandpass => 2.0 * (y1 - y2),
            Response::Notch => u - 2.0 * (y1 - y2),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct SallenKeyCoeffs {
    /// Whether the 12 dB stage and the 24 dB pair run.
    runs: [bool; 2],
    db12: StageCoeffs,
    db24: [StageCoeffs; 2],
    /// Gains after each of the pair's sections.
    gain24: [f32; 2],
}

#[derive(Clone, Copy, Default)]
pub struct SallenKeyState {
    db12: Stage,
    db24: [Stage; 2],
}

#[derive(Clone, Copy, Default)]
pub struct SallenKey;

impl Kernel for SallenKey {
    type Response = Response;
    /// Whether resonance is a fixed value, not a signal that can move.
    type Options = bool;
    type Coeffs = SallenKeyCoeffs;
    type State = SallenKeyState;
    const VOLTS: f32 = 12.0;

    fn coeffs(
        response: Response,
        cutoff: f32,
        resonance: f32,
        rates: Rates,
        taps: Taps,
        steady_resonance: bool,
    ) -> SallenKeyCoeffs {
        let k = loop_gain(resonance, SELF_OSCILLATION);
        let g = rates.gain(cutoff);
        let stage = |octaves: f32, k: f32| {
            let g = if octaves == 0.0 {
                g
            } else {
                rates.gain(cutoff + octaves)
            };
            StageCoeffs::new(g, k)
        };
        // The 24 dB pair's sections, their gains, and the feedback of the
        // more resonant one.
        let (db24, gain24, pair_k) = match response {
            Response::Notch => (
                [
                    stage(-NOTCH_SPREAD_OCTAVES, k),
                    stage(NOTCH_SPREAD_OCTAVES, k),
                ],
                [1.0; 2],
                k,
            ),
            Response::Bandpass => {
                let q = (1.0 / (SELF_OSCILLATION - k).max(1.0 / MAX_PAIR_Q)).max(0.5);
                let (section_q, octaves, gain) = bandpass_pair(q);
                let section_k = SELF_OSCILLATION - 1.0 / section_q
                    + (k - (SELF_OSCILLATION - 1.0 / MAX_PAIR_Q)).max(0.0);
                (
                    [stage(-octaves, section_k), stage(octaves, section_k)],
                    // The first section at unity, so the pair peaks as high
                    // as the 12 dB stage does.
                    [0.5 / section_q, gain * q / section_q],
                    section_k,
                )
            }
            Response::Lowpass | Response::Highpass => {
                let rise =
                    (SELF_OSCILLATION - BUTTERWORTH_FEEDBACK_B) / RESONANCE_AT_SELF_OSCILLATION;
                let resonant_k = BUTTERWORTH_FEEDBACK_B + rise * resonance.clamp(0.0, 5.0);
                (
                    [stage(0.0, BUTTERWORTH_FEEDBACK_A), stage(0.0, resonant_k)],
                    [1.0; 2],
                    resonant_k,
                )
            }
        };
        // Unread, a section stands still only while it would settle again
        // within moments of being read: at a fixed resonance below the
        // point where it rings on.
        let pauses = |k: f32| steady_resonance && k < RINGS_ON;
        SallenKeyCoeffs {
            runs: [taps[DB12] || !pauses(k), taps[DB24] || !pauses(pair_k)],
            db12: stage(0.0, k),
            db24,
            gain24,
        }
    }

    #[inline]
    fn step(
        response: Response,
        c: &SallenKeyCoeffs,
        state: &mut SallenKeyState,
        x: f32,
        taps: Taps,
    ) -> [f32; 4] {
        let tapped = [taps[DB12], taps[DB24]];
        let db12 = if c.runs[0] {
            state.db12.step(&c.db12, response, x)
        } else {
            0.0
        };
        let db24 = if c.runs[1] {
            let mid = c.gain24[0] * state.db24[0].step(&c.db24[0], response, x);
            c.gain24[1] * state.db24[1].step(&c.db24[1], response, mid)
        } else {
            0.0
        };
        let [db12, db24] = [
            if tapped[0] { db12 } else { 0.0 },
            if tapped[1] { db24 } else { 0.0 },
        ];
        [0.0, db12, 0.0, db24]
    }
}
