//! Quantization-noise grit effect.
//!
//! Recreates the bright, granular digital fizz that a coarsely-quantized signal
//! produces when its quantization error is differentiated. The signal is snapped to a coarse voltage grid (with dither
//! so the result is broadband noise rather than a tonal staircase buzz), the
//! quantized value is differentiated to brighten it, and that grit is added back
//! onto the dry signal.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::rng::{LcgRng, seed_base};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct QuantNoiseParams {
    #[input(
        "signal to grit up",
        type = audio,
    )]
    input: PolySignal,
    #[input(
        "how much grit is added: 0 clean, higher more fizz",
        type = control,
        default = 1.0,
        range = (0.0, ..),
        control_range = (0.0, 5.0),
    )]
    amount: PolySignal,
    #[input(
        "size of the voltage grid the signal snaps to, in volts; bigger steps give coarser grit",
        type = control,
        default = 0.25,
        range = (0.0, ..),
        control_range = (0.0, 5.0),
    )]
    #[deserr(default)]
    step: Option<PolySignal>,
    #[input(
        "dither depth: 0 a tonal staircase buzz that tracks the signal, 5 broadband noise",
        type = control,
        default = 5.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    dither: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct QuantNoiseOutputs {
    #[output("output", "the input with grit added", type = cv, default)]
    sample: PolyOutput,
}

/// Smallest allowed quantization step, in volts. Guards the divide and keeps the
/// grid from collapsing onto a single level (which would emit silence).
const MIN_STEP: f32 = 1.0e-4;

#[derive(Default, Clone, Copy)]
struct LaneState {
    amount: SwapGlide,
    step: SwapGlide,
    /// Previous quantized value, for the differentiation that brightens the grit.
    prev_q: f32,
    /// False until the first sample has seeded `prev_q`. A fresh lane primes
    /// itself so the differentiation does not spike on the opening sample (which
    /// would click); a lane carried over by `transfer_state_from` arrives
    /// already primed and keeps its running `prev_q`.
    primed: bool,
    /// Per-lane dither source.
    rng: LcgRng,
}

/// Module-level state for the quantization-noise effect.
#[derive(Default)]
struct QuantNoiseState {
    /// Set once the per-lane dither RNGs have been seeded off the module's
    /// stable heap address (see `seed`).
    seeded: bool,
}

const QUANT_NOISE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Digital grit",
    description: "Adds a bright, granular digital fizz on top of the signal. The grit follows \
                  the signal's own level and motion, so it crackles on loud, moving material and \
                  fades with it: a lo-fi edge for pads, drums or bass.",
    categories: &[ModuleCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "Fizzy lo-fi pad",
        code: "$saw($chord('A2[m9]')).lpf('a4', 1).quantNoise(2).out()",
    }],
};

#[module(
    name = "$quantNoise",
    docs = QUANT_NOISE_DOCS,
    args(input, amount),
    patch_update
)]
pub struct QuantNoise {
    outputs: QuantNoiseOutputs,
    state: QuantNoiseState,
    lane_state: Box<[LaneState]>,
    params: QuantNoiseParams,
}

impl QuantNoise {
    /// Seed each lane's dither RNG off this module's stable heap address so
    /// every instance gets a distinct stream; mixing the lane index with an
    /// odd golden-ratio constant decorrelates the lanes.
    ///
    /// Called from `on_patch_update`, not `init`: `init` runs while the module
    /// still lives in a transient stack slot that is reused across
    /// constructions, so every instance would capture the same address and seed
    /// identically. By `on_patch_update` the module sits at its stable
    /// per-instance heap address.
    ///
    /// Guarded by `seeded` (which, with the RNG state, rides
    /// `transfer_state_from`): `on_patch_update` runs after the state swap, so
    /// re-seeding unconditionally would clobber the stream carried over from the
    /// previous patch on every edit. Seeding once keeps the dither continuous.
    fn seed(&mut self) {
        let base = seed_base(self);
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            state.rng.seed(base, lane);
        }
        self.state.seeded = true;
    }

    fn update(&mut self, sample_rate: f32) {
        let num_lanes = self.lane_count();

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];

            let input = self.params.input.get_value(lane);
            state.amount.update(
                self.params.amount.get_value(lane),
                self.params.amount.glide_mark(lane),
                sample_rate,
            );
            state.step.update(
                self.params.step.value_or(lane, 0.25),
                self.params.step.glide_mark(lane),
                sample_rate,
            );
            let amount = (*state.amount).max(0.0);
            let step = (*state.step).max(MIN_STEP);
            let dither = self.params.dither.value_or(lane, 5.0).clamp(0.0, 5.0) / 5.0;

            // Dither up to ±one step so rounding can flip to an adjacent level
            // anywhere in the grid, decorrelating the quantization error into
            // broadband noise rather than a tonal staircase.
            let d = state.rng.next_bipolar() * step * dither;
            let q = ((input + d) / step).round() * step;

            // Prime on the first sample so the difference below starts from the
            // current level instead of zero (which would emit an onset click).
            if !state.primed {
                state.prev_q = q;
                state.primed = true;
            }

            // Differentiate the quantized value to brighten the grit (this is the
            // high-pass that gives the fizz its characteristic bright timbre),
            // then scale and add it back onto the dry signal.
            let grit = (q - state.prev_q) * amount;
            state.prev_q = q;

            self.outputs.sample.set(lane, input + grit);
        }
    }
}

impl crate::types::PatchUpdateHandler for QuantNoise {
    fn on_patch_update(&mut self) {
        if !self.state.seeded {
            self.seed();
        }
    }
}

message_handlers!(impl QuantNoise {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, PatchUpdateHandler, Signal};

    fn make(input: f32, amount: f32, step: f32, dither: f32) -> QuantNoise {
        let mut outputs = QuantNoiseOutputs::default();
        outputs.set_all_lanes(1);
        let mut m = QuantNoise {
            params: QuantNoiseParams {
                input: PolySignal::mono(Signal::Volts(input)),
                amount: PolySignal::mono(Signal::Volts(amount)),
                step: Some(PolySignal::mono(Signal::Volts(step))),
                dither: Some(PolySignal::mono(Signal::Volts(dither))),
            },
            outputs,
            _lane_count: 1,
            _channels: 1,
            _block_index: Default::default(),
            state: QuantNoiseState::default(),
            lane_state: vec![LaneState::default(); 1].into_boxed_slice(),
        };
        m.on_patch_update();
        m
    }

    /// Run `n` samples, returning lane-0 output minus the (constant) dry input
    /// so the result is the grit alone.
    fn grit_series(m: &mut QuantNoise, input: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|_| {
                m.update(48000.0);
                m.outputs.sample.get(0) - input
            })
            .collect()
    }

    #[test]
    fn amount_zero_is_clean_passthrough() {
        let mut m = make(2.0, 0.0, 0.25, 1.0);
        for _ in 0..500 {
            m.update(48000.0);
            assert_eq!(
                m.outputs.sample.get(0),
                2.0,
                "amount 0 must pass dry signal"
            );
        }
    }

    #[test]
    fn dithered_constant_input_produces_broadband_grit() {
        // A constant input would sit on one quantization level (silent grit)
        // without dither; with dither it must produce a bright, bipolar,
        // non-trivial grit signal.
        let mut m = make(2.0, 1.0, 0.25, 5.0);
        let grit = grit_series(&mut m, 2.0, 4000);
        let rms = (grit.iter().map(|g| g * g).sum::<f32>() / grit.len() as f32).sqrt();
        assert!(rms > 1.0e-3, "dithered grit should be audible, rms={rms}");
        // Bright = high-pass: lag-1 autocorrelation should be negative.
        let mean = grit.iter().sum::<f32>() / grit.len() as f32;
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for i in 1..grit.len() {
            num += (grit[i] - mean) * (grit[i - 1] - mean);
            den += (grit[i] - mean).powi(2);
        }
        assert!(
            num / den < 0.0,
            "grit should be bright (high-pass), r1={}",
            num / den
        );
    }

    #[test]
    fn no_dither_constant_input_is_silent() {
        // With dither off, a constant input lands on a fixed grid level every
        // sample, so the differentiated grit is exactly zero.
        let mut m = make(2.0, 1.0, 0.25, 0.0);
        let grit = grit_series(&mut m, 2.0, 500);
        assert!(
            grit.iter().all(|&g| g == 0.0),
            "undithered constant input should yield no grit"
        );
    }

    #[test]
    fn output_is_finite() {
        let mut m = make(0.0, 5.0, 5.0, 1.0);
        for n in 0..2000 {
            let x = (n as f32 * 0.01).sin() * 5.0;
            m.params.input = PolySignal::mono(Signal::Volts(x));
            m.update(48000.0);
            assert!(m.outputs.sample.get(0).is_finite());
        }
    }
}
