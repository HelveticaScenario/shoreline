//! DC-blocking high-pass filter module.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::dc_blocker::{DEFAULT_DC_BLOCK_FC_HZ, DcBlocker};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct FilterDcParams {
    #[input("signal to strip of DC offset and sub-audio rumble", type = audio)]
    input: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct FilterDcOutputs {
    #[output("output", "the input, centred on 0 V", type = cv, default)]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct LaneState {
    /// One-pole high-pass state — `y[n] = x[n] - x[n-1] + R · y[n-1]`.
    dc_blocker: DcBlocker,
}

/// Module-level state holding the sample-rate-derived constant.
#[derive(Default)]
struct FilterDcState {
    dc_block_coeff: f32,
}

const FILTER_DC_DOCS: ModuleDocs = ModuleDocs {
    summary: "DC offset remover",
    description: "Recentres a signal on 0 V by removing DC offset and sub-audio rumble below \
                  about 20 Hz, leaving everything audible untouched. Put it after asymmetric \
                  distortion or offset modulation so the signal sits centred and keeps its \
                  headroom.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Recentring tube-style drive",
        code: "$saw('c2').overdrive(4, { mode: 'asym' }).filterDC().out()",
    }],
};

#[module(name = "$filterDC", docs = FILTER_DC_DOCS, args(input), has_init)]
pub struct FilterDc {
    outputs: FilterDcOutputs,
    state: FilterDcState,
    lane_state: Box<[LaneState]>,
    params: FilterDcParams,
}

impl FilterDc {
    /// Compute the sample-rate-dependent filter coefficient once at construction.
    /// Invoked by the `#[module]` proc macro on the main thread. Sample-rate-only,
    /// so it stays correct even though `transfer_state_from` swaps the whole
    /// `state` struct on a patch update.
    fn init(&mut self, sample_rate: f32) {
        self.state.dc_block_coeff = DcBlocker::coeff(DEFAULT_DC_BLOCK_FC_HZ, sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let num_lanes = self.lane_count();
        let dc_coeff = self.state.dc_block_coeff;

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];
            let input = self.params.input.get_value(lane);
            let out = state.dc_blocker.process(input, dc_coeff);
            self.outputs.sample.set(lane, out);
        }
    }
}

message_handlers!(impl FilterDc {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, Signal};

    fn make(input: f32) -> FilterDc {
        let mut outputs = FilterDcOutputs::default();
        outputs.set_all_lanes(1);
        let mut filter = FilterDc {
            params: FilterDcParams {
                input: PolySignal::mono(Signal::Volts(input)),
            },
            outputs,
            _lane_count: 1,
            _block_index: Default::default(),
            state: FilterDcState::default(),
            lane_state: vec![LaneState::default(); 1].into_boxed_slice(),
        };
        filter.init(48000.0);
        filter
    }

    #[test]
    fn blocks_constant_offset() {
        let mut filter = make(1.0);
        let mut last = 0.0;
        for _ in 0..4000 {
            filter.update(48000.0);
            last = filter.outputs.sample.get(0);
        }
        assert!(last.abs() < 0.05, "DC should be blocked, got {last}");
    }

    #[test]
    fn passes_audio_rate_signal() {
        use std::f32::consts::PI;
        let mut filter = make(0.0);
        // ~480 Hz at 48 kHz, well above the 20 Hz corner.
        let freq_norm = 0.01_f32;
        let mut peak = 0.0_f32;
        for n in 0..4000 {
            let x = (2.0 * PI * freq_norm * n as f32).sin();
            filter.params.input = PolySignal::mono(Signal::Volts(x));
            filter.update(48000.0);
            if n >= 1000 {
                peak = peak.max(filter.outputs.sample.get(0).abs());
            }
        }
        assert!(peak > 0.5, "AC signal should pass, got peak {peak}");
    }
}
