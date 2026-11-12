use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use super::glided_operand::{self, Combine};
use crate::dsp::utils::Sample;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ShiftParams {
    #[input("signal to offset", type = audio)]
    input: PolySignal,
    #[input(
        "offset added to the signal, in volts",
        type = control,
        default = 0.0,
        control_range = (-5.0, 5.0),
    )]
    offset: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ShiftOutputs {
    #[output("output", "the offset signal", type = cv, precision = inherit, default)]
    sample: PolyOutput,
}

const SHIFT_DOCS: ModuleDocs = ModuleDocs {
    summary: "DC offset",
    description: "Adds an offset to a signal, sample by sample, gliding when the \
                  offset is edited.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "LFO raised to sit above 0 V",
        code: "$saw('c2').lpf($sine('0.5hz').shift(2), 2).out()",
    }],
};

#[module(name = "$shift", docs = SHIFT_DOCS, args(input, offset), block)]
pub struct Shift {
    outputs: ShiftOutputs,
    params: ShiftParams,
    channel_state: Box<[SwapGlide]>,
}

struct Offset;

impl Combine for Offset {
    #[inline(always)]
    fn apply<T: Sample>(input: T, offset: T) -> T {
        input + offset
    }
}

impl Shift {
    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();
        glided_operand::update::<Offset>(
            &self.params.input,
            &self.params.offset,
            &mut self.channel_state,
            &mut self.outputs.sample,
            channels,
            sample_rate,
        );
    }

    fn process_block(
        &mut self,
        outputs: &mut ShiftBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        glided_operand::process_block::<Offset>(
            &self.params.input,
            &self.params.offset,
            &mut self.channel_state,
            &mut outputs.sample,
            range,
            channels,
        )
    }
}

message_handlers!(impl Shift {});
