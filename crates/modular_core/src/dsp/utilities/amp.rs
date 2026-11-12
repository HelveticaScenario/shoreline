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
struct AmpParams {
    #[input("signal to turn up or down", type = audio)]
    input: PolySignal,
    #[input(
        "linear gain: 5 unity, 0 silence, 10 double, negative inverts",
        type = control,
        default = 5.0,
        control_range = (0.0, 10.0),
    )]
    factor: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AmpOutputs {
    #[output("output", "the scaled signal", type = cv, precision = inherit, default)]
    sample: PolyOutput,
}

const AMP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Linear gain",
    description: "Multiplies a signal by a gain where 5 V is unity, sample by \
                  sample.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Tremolo",
        code: "$saw('c3').amp($sine('5hz').range(0, 5)).out()",
    }],
};

#[module(name = "$amp", docs = AMP_DOCS, args(input, factor), block)]
pub struct Amp {
    outputs: AmpOutputs,
    params: AmpParams,
    channel_state: Box<[SwapGlide]>,
}

struct Scale;

impl Combine for Scale {
    #[inline(always)]
    fn apply<T: Sample>(input: T, factor: T) -> T {
        input * (factor / T::from_f32(5.0))
    }
}

impl Amp {
    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();
        glided_operand::update::<Scale>(
            &self.params.input,
            &self.params.factor,
            &mut self.channel_state,
            &mut self.outputs.sample,
            channels,
            sample_rate,
        );
    }

    fn process_block(
        &mut self,
        outputs: &mut AmpBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        glided_operand::process_block::<Scale>(
            &self.params.input,
            &self.params.factor,
            &mut self.channel_state,
            &mut outputs.sample,
            range,
            channels,
        )
    }
}

message_handlers!(impl Amp {});
