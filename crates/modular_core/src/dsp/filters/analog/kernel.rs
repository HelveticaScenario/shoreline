//! What a filter topology provides to the runner, and the parts the
//! four-stage loops share.

use crate::dsp::filters::resonant_svf::{Response, prewarped_gain};

/// Resonance setting at which a loop that self-oscillates reaches it.
pub(crate) const RESONANCE_AT_SELF_OSCILLATION: f32 = 4.5;

/// The engine's sample rate and the rate the kernel runs at.
#[derive(Clone, Copy)]
pub(crate) struct Rates {
    pub engine: f32,
    pub core: f32,
}

impl Rates {
    /// The trapezoidal integrator gain for a cutoff in V/Oct.
    pub(crate) fn gain(self, cutoff: f32) -> f32 {
        prewarped_gain(cutoff, self.engine, self.core)
    }
}

/// The loop gain for a resonance setting, given the gain at which the loop
/// self-oscillates.
pub(crate) fn loop_gain(resonance: f32, self_oscillation: f32) -> f32 {
    resonance.clamp(0.0, 5.0) / RESONANCE_AT_SELF_OSCILLATION * self_oscillation
}

/// Which of a filter's outputs something reads: `[6, 12, 18, 24]` dB.
pub(crate) type Taps = [bool; 4];

/// The 12 and 24 dB outputs' places in [`Taps`].
pub(crate) const DB12: usize = 1;
pub(crate) const DB24: usize = 3;

/// One filter topology.
pub(crate) trait Kernel: Copy + Default {
    type Response: Copy;
    /// Non-signal params of the topology's own.
    type Options: Copy + PartialEq;
    type Coeffs: Copy + Default;
    type State: Copy + Default;
    /// Volts per unit of the kernel's input and outputs.
    const VOLTS: f32;

    /// Coefficients for the outputs in `taps`.
    fn coeffs(
        response: Self::Response,
        cutoff: f32,
        resonance: f32,
        rates: Rates,
        taps: Taps,
        options: Self::Options,
    ) -> Self::Coeffs;

    /// One sample at the core rate of every output. An output outside
    /// `taps` reads 0. State that feeds it alone stands still unless it
    /// rings on or self-oscillates, whose phase a listener would hear
    /// restart.
    fn step(
        response: Self::Response,
        coeffs: &Self::Coeffs,
        state: &mut Self::State,
        x: f32,
        taps: Taps,
    ) -> [f32; 4];
}

/// Weights on a four-stage loop's input and its four stage voltages.
pub(crate) type Mix = [f32; 5];

/// The mix of an output a response does not have.
pub(crate) const UNUSED: Mix = [0.0; 5];

/// Coefficients of a four-stage loop whose outputs are mixes of its input
/// and stage voltages.
#[derive(Clone, Copy)]
pub(crate) struct LoopCoeffs {
    /// Integrator gain.
    pub f: f32,
    /// Loop gain.
    pub k: f32,
    mixes: [Mix; 4],
    gain: f32,
    /// Whether a mix's first weight is on the filter's input in place of
    /// the loop's.
    reads_input: bool,
}

impl Default for LoopCoeffs {
    fn default() -> Self {
        Self {
            f: 0.0,
            k: 0.0,
            mixes: [UNUSED; 4],
            gain: 1.0,
            reads_input: false,
        }
    }
}

impl LoopCoeffs {
    /// `mixes` are the 6, 12, 18 and 24 dB outputs with the loop open. A
    /// lowpass gets back half of the passband level the loop takes away as
    /// resonance rises. An allpass reads the filter's input in place of the
    /// loop's, which holds the level of its 24 dB output at every resonance
    /// and of its 12 dB output away from the cutoff.
    pub(crate) fn new(
        f: f32,
        resonance: f32,
        self_oscillation: f32,
        mixes: [Mix; 4],
        response: LoopResponse,
    ) -> Self {
        let k = loop_gain(resonance, self_oscillation);
        Self {
            f,
            k,
            mixes,
            gain: match response {
                LoopResponse::Base(Response::Lowpass) => 1.0 + 0.5 * k,
                _ => 1.0,
            },
            reads_input: matches!(response, LoopResponse::Allpass),
        }
    }

    /// Each output in `taps` as its mix of the filter input `x` or loop
    /// input `u` and the stage voltages `v`; the others read 0.
    #[inline]
    pub(crate) fn outputs(&self, x: f32, u: f32, v: &[f32; 4], taps: Taps) -> [f32; 4] {
        let input = if self.reads_input { x } else { u };
        std::array::from_fn(|tap| {
            if taps[tap] {
                let mix = &self.mixes[tap];
                self.gain
                    * (mix[0] * input
                        + mix[1] * v[0]
                        + mix[2] * v[1]
                        + mix[3] * v[2]
                        + mix[4] * v[3])
            } else {
                0.0
            }
        })
    }
}

/// The responses a four-stage loop is mixed into.
#[derive(Clone, Copy)]
pub(crate) enum LoopResponse {
    Base(Response),
    Allpass,
}
