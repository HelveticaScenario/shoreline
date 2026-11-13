//! Triangle segment morphing effect module.
//!
//! Adapted from the 4ms Ensemble Oscillator warp mode.
//! Copyright 4ms Company. Used under GPL v3.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::fx::enosc_tables::{aa_segment, interpolate_segment};
use crate::dsp::utils::voct_to_hz;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SegmentParams {
    #[input(
        "signal to shape",
        type = audio,
        range = (-5.0, 5.0),
    )]
    input: PolySignal,
    #[input(
        "morphs through 8 shapes: 0 clean, higher squares off and then ripples",
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
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SegmentOutputs {
    #[output("output", "shaped signal", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct LaneState {
    amount: SwapGlide,
}

const SEGMENT_DOCS: ModuleDocs = ModuleDocs {
    summary: "Morphing waveshaper",
    description: "Morphs a signal through eight shapes as `amount` rises: clean at first, then \
                  squashed and squared off, then stepped and rippling with overtones. Works best \
                  on simple waveforms like sines and triangles.",
    categories: &[ModuleCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "Slowly morphing sine",
        code: "$sine('c3').segment($sine('0.1hz').range(0, 5)).out()",
    }],
};

#[module(name = "$segment", docs = SEGMENT_DOCS, args(input, amount))]
pub struct Segment {
    outputs: SegmentOutputs,
    lane_state: Box<[LaneState]>,
    params: SegmentParams,
}

impl Segment {
    fn update(&mut self, sample_rate: f32) {
        let num_lanes = self.lane_count();
        let freq_connected = !self.params.freq.is_disconnected();

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
                aa_segment(freq_hz / sample_rate, amount_norm)
            } else {
                amount_norm
            };

            // Normalize input from typical [-5, 5] range to [-1, 1]
            let input_norm = (input / 5.0).clamp(-1.0, 1.0);

            // Apply segment morphing
            let shaped = interpolate_segment(input_norm, amount_norm);

            // Scale back to output range
            self.outputs.sample.set(lane, shaped * 5.0);
        }
    }
}

message_handlers!(impl Segment {});
