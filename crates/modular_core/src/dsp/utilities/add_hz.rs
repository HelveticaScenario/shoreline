use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input::{self, InputScratch};
use crate::dsp::utils::{hz_to_voct, voct_to_hz};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use crate::types::SwapGlide;

/// Lower bound for the post-offset frequency, keeping `hz_to_voct` away from log2(≤0).
const MIN_HZ: f32 = 1e-4;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct AddHzParams {
    #[input("pitch to detune", type = pitch)]
    input: PolySignal,
    #[input(
        "frequency added to the pitch, in Hz; small amounts give slow beating",
        type = control,
        control_range = (-5.0, 5.0),
    )]
    offset: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AddHzOutputs {
    #[output("output", "the detuned pitch", type = cv, default)]
    sample: PolyOutput,
}

const ADD_HZ_DOCS: ModuleDocs = ModuleDocs {
    summary: "Detune a pitch by Hz",
    description: "Shifts a pitch by a fixed number of Hz rather than by cents, so a \
                  detuned copy beats at the same steady rate whatever note plays. \
                  Layer it against the original for slow, even chorusing.",
    categories: &[ModuleCategory::Pitch],
    sections: &[],
    examples: &[DocExample {
        title: "Slow, even beating",
        code: r#"
            const pitch = $p('c3 eb3 g3 bb2')
            $saw(pitch).out()
            $saw($addHz(pitch, 0.7)).out()
        "#,
    }],
};

#[module(name = "$addHz", docs = ADD_HZ_DOCS, args(input, offset), block)]
pub struct AddHz {
    outputs: AddHzOutputs,
    params: AddHzParams,
    lane_state: Box<[SwapGlide]>,
}

/// The V/Oct pitch `voct` moved by `offset` Hz.
#[inline(always)]
fn add_hz(voct: f32, offset: f32) -> f32 {
    hz_to_voct((voct_to_hz(voct) + offset).max(MIN_HZ))
}

impl AddHz {
    fn update(&mut self, sample_rate: f32) {
        let offset = &self.params.offset;
        for i in 0..self.lane_count() {
            let offset_value =
                self.lane_state[i].follow(offset.get_value(i), offset.glide_mark(i), sample_rate);
            let value = add_hz(self.params.input.get_value(i), offset_value);
            self.outputs.sample.set(i, value);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut AddHzBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let glides = &mut self.lane_state[..lanes];
        let offset = &self.params.offset;
        if offset.glides_pending(glides) {
            return None;
        }
        let (mut s0, mut s1) = (InputScratch::new(), InputScratch::new());
        let input = self
            .params
            .input
            .block_input(range.clone(), lanes, &mut s0)?;
        let offset_input = offset.block_input(range.clone(), lanes, &mut s1)?;
        offset.settle_glides(glides, offset_input.row(range.end - 1));
        block_input::map(
            &mut outputs.sample,
            range,
            [input, offset_input],
            |[x, offset]| add_hz(x, offset),
        );
        Some(())
    }
}

message_handlers!(impl AddHz {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::utils::C4_HZ_F32;
    use crate::types::{OutputStruct, Signal};

    fn make(input_voct: f32, offset_hz: f32) -> AddHz {
        let mut outputs = AddHzOutputs::default();
        outputs.set_all_lanes(1);
        AddHz {
            params: AddHzParams {
                input: PolySignal::mono(Signal::Volts(input_voct)),
                offset: PolySignal::mono(Signal::Volts(offset_hz)),
            },
            outputs,
            _lane_count: 1,
            _channels: 1,
            _block_index: Default::default(),
            lane_state: Box::new([SwapGlide::default()]),
        }
    }

    #[test]
    fn adds_hz_offset() {
        // C4 (0 V) with no offset stays at 0 V.
        let mut m = make(0.0, 0.0);
        m.update(48000.0);
        assert!(m.outputs.sample.get(0).abs() < 1e-5);

        // Adding C4's own frequency doubles it → up exactly one octave (+1 V).
        let mut m = make(0.0, C4_HZ_F32);
        m.update(48000.0);
        assert!((m.outputs.sample.get(0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn floors_non_positive_frequency() {
        // A large negative offset drives frequency ≤ 0; output must stay finite.
        let mut m = make(0.0, -10_000.0);
        m.update(48000.0);
        assert!(m.outputs.sample.get(0).is_finite());
    }
}
