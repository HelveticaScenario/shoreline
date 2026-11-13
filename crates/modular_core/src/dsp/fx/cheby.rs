//! Chebyshev polynomial waveshaping effect module.
//!
//! Adapted from the 4ms Ensemble Oscillator warp mode.
//! Copyright 4ms Company. Used under GPL v3.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::fx::enosc_tables::{aa_cheby, interpolate_cheby};
use crate::dsp::utils::dc_blocker::{DEFAULT_DC_BLOCK_FC_HZ, DcBlocker};
use crate::dsp::utils::fade::{DEFAULT_FADE_IN_SECS, FadeIn};
use crate::dsp::utils::voct_to_hz;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ChebyParams {
    #[input(
        "signal to shape",
        type = audio,
        range = (-5.0, 5.0),
    )]
    input: PolySignal,
    #[input(
        "harmonic richness: 0 clean, 5 dominated by the highest harmonics",
        type = control,
        range = (0.0, 5.0),
    )]
    amount: PolySignal,
    #[input(
        "pitch of the incoming signal; patch it to reduce aliasing on high notes",
        type = pitch,
    )]
    #[deserr(default)]
    freq: Option<PolySignal>,
    #[input("remove the DC offset the even harmonics add, keeping the output centred")]
    #[serde(rename = "blockDC", default = "default_true")]
    #[deserr(rename = "blockDC", default = default_true())]
    block_dc: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ChebyOutputs {
    #[output("output", "waveshaped signal", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct LaneState {
    amount: SwapGlide,
    /// DC blocker (one-pole high-pass) on the output — the even-order harmonics
    /// introduce a DC offset this removes.
    dc_blocker: DcBlocker,
    /// Onset fade-in — suppresses the click when the module is first added
    /// (`amount` snaps to its full value on the first sample, so the output
    /// would otherwise jump from silence).
    fade: FadeIn,
}

/// Module-level state holding the sample-rate-derived constants.
#[derive(Default)]
struct ChebyState {
    dc_block_coeff: f32,
    /// Per-sample increment for the onset fade-in.
    fade_inc: f32,
}

const CHEBY_DOCS: ModuleDocs = ModuleDocs {
    summary: "Harmonic waveshaper",
    description: "Adds overtones to a signal, from a clean pass at 0 to a bright, buzzy stack of \
                  upper harmonics at full. Sounds smoothest on sines and triangles.",
    categories: &[ModuleCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "Bass that brightens and darkens",
        code: r#"
            const bass = $p('c2 c2 eb2 g1')
            $sine(bass)
              .cheby($sine('0.25hz').range(0, 4), { freq: bass })
              .out()
        "#,
    }],
};

#[module(name = "$cheby", docs = CHEBY_DOCS, args(input, amount), has_init)]
pub struct Cheby {
    outputs: ChebyOutputs,
    state: ChebyState,
    lane_state: Box<[LaneState]>,
    params: ChebyParams,
}

impl Cheby {
    /// Compute the sample-rate-dependent constants once at construction.
    /// Invoked by the `#[module]` proc macro on the main thread. Both values are
    /// sample-rate-only, so they stay correct even though `transfer_state_from`
    /// swaps the whole `state` struct on a patch update.
    fn init(&mut self, sample_rate: f32) {
        self.state.dc_block_coeff = DcBlocker::coeff(DEFAULT_DC_BLOCK_FC_HZ, sample_rate);
        self.state.fade_inc = FadeIn::increment(DEFAULT_FADE_IN_SECS, sample_rate);
    }

    fn update(&mut self, sample_rate: f32) {
        let num_lanes = self.lane_count();
        let freq_connected = !self.params.freq.is_disconnected();
        let block_dc = self.params.block_dc;
        let dc_coeff = self.state.dc_block_coeff;
        let fade_inc = self.state.fade_inc;

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];

            let input = self.params.input.get_value(lane);
            let amount_raw = self.params.amount.get_value(lane);

            state
                .amount
                .update(amount_raw, self.params.amount.glide_mark(lane), sample_rate);
            let amount = *state.amount;

            // Normalize amount from [0, 5] to [0, 1] for table lookup
            let amount_norm = (amount / 5.0).clamp(0.0, 1.0);

            // Apply anti-aliasing when freq is connected
            let amount_norm = if freq_connected {
                let freq_hz = voct_to_hz(self.params.freq.value_or_zero(lane));
                aa_cheby(freq_hz / sample_rate, amount_norm)
            } else {
                amount_norm
            };

            // Normalize input from typical [-5, 5] range to [-1, 1]
            let input_norm = (input / 5.0).clamp(-1.0, 1.0);

            // Apply Chebyshev waveshaping
            let shaped = interpolate_cheby(input_norm, amount_norm);

            // DC blocker on the output — the even-order harmonics introduce a
            // DC offset that a one-pole high-pass (~20 Hz) removes. Bypassed
            // when `block_dc` is false.
            let out = if block_dc {
                state.dc_blocker.process(shaped, dc_coeff)
            } else {
                shaped
            };

            // Fade in over the first few ms to suppress the onset click when
            // the module is first added.
            let fade = state.fade.advance(fade_inc);

            // Scale back to output range
            self.outputs.sample.set(lane, out * 5.0 * fade);
        }
    }
}

message_handlers!(impl Cheby {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, Signal};

    fn make(input: f32, amount: f32, block_dc: bool) -> Cheby {
        let mut outputs = ChebyOutputs::default();
        outputs.set_all_lanes(1);
        let mut cheby = Cheby {
            params: ChebyParams {
                input: PolySignal::mono(Signal::Volts(input)),
                amount: PolySignal::mono(Signal::Volts(amount)),
                freq: None,
                block_dc,
            },
            outputs,
            _lane_count: 1,
            _block_index: Default::default(),
            state: ChebyState::default(),
            lane_state: vec![LaneState::default(); 1].into_boxed_slice(),
        };
        cheby.init(48000.0);
        cheby
    }

    #[test]
    fn dc_blocker_removes_constant_offset() {
        // A constant (DC) input produces a constant shaped output; with the
        // blocker enabled it must decay toward zero.
        let mut cheby = make(3.0, 5.0, true);
        let mut last = 0.0;
        for _ in 0..8000 {
            cheby.update(48000.0);
            last = cheby.outputs.sample.get(0);
        }
        assert!(
            last.abs() < 0.05,
            "expected near 0 V after DC blocker, got {last}"
        );
    }

    #[test]
    fn block_dc_false_does_not_filter() {
        // With blocking disabled the output is a pure waveshaping of the
        // constant input: it holds steady at a non-zero DC level instead of
        // decaying, proving the parameter gates the filter.
        let mut cheby = make(3.0, 5.0, false);
        for _ in 0..8000 {
            cheby.update(48000.0);
        }
        let settled = cheby.outputs.sample.get(0);
        for _ in 0..2000 {
            cheby.update(48000.0);
        }
        let later = cheby.outputs.sample.get(0);
        assert!(
            settled.abs() > 1e-3,
            "shaper should produce a non-zero DC level for this input, got {settled}"
        );
        assert!(
            (settled - later).abs() < 1e-4,
            "bypassed output should hold steady, got {settled} then {later}"
        );
    }

    #[test]
    fn fades_in_on_fresh_add() {
        // A freshly added module with a non-zero amount must start near silence
        // and ramp up, rather than jumping straight to full level (the click the
        // user reported). `block_dc` off so a constant input keeps a steady
        // settled level to compare against.
        let mut cheby = make(3.0, 1.28, false);
        cheby.update(48000.0);
        let first = cheby.outputs.sample.get(0).abs();
        for _ in 0..8000 {
            cheby.update(48000.0);
        }
        let settled = cheby.outputs.sample.get(0).abs();
        assert!(
            settled > 1e-3,
            "settled output should be non-zero, got {settled}"
        );
        assert!(
            first < settled * 0.1,
            "onset should be faded in (first {first} should be well below settled {settled})"
        );
    }

    #[test]
    fn carried_over_lane_does_not_refade() {
        // A lane whose state was carried over by `transfer_state_from`
        // arrives with `fade` already at 1.0, so it must play at full level
        // immediately instead of re-fading on a patch edit.
        let mut cheby = make(3.0, 1.28, false);
        cheby.lane_state[0].fade.gain = 1.0;
        cheby.update(48000.0);
        let first = cheby.outputs.sample.get(0).abs();
        for _ in 0..8000 {
            cheby.update(48000.0);
        }
        let settled = cheby.outputs.sample.get(0).abs();
        assert!(
            settled > 1e-3,
            "settled output should be non-zero, got {settled}"
        );
        assert!(
            (first - settled).abs() < 1e-4,
            "carried-over lane should not re-fade (first {first} vs settled {settled})"
        );
    }
}
