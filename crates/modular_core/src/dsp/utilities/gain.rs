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
struct GainParams {
    #[input("signal to turn up or down", type = audio)]
    input: PolySignal,
    #[input(
        "perceptual level: 5 unity, 0 silence, each halving about -18 dB",
        type = control,
        default = 5.0,
        control_range = (0.0, 10.0),
    )]
    level: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GainOutputs {
    #[output("output", "the scaled signal", type = cv, default)]
    sample: PolyOutput,
}

const GAIN_DOCS: ModuleDocs = ModuleDocs {
    summary: "Perceptual gain",
    description: "Scales a signal by `(level / 5)³`, an audio-taper curve on which \
                  equal steps of `level` sound like equal steps of loudness. Outs apply \
                  their gain through it.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Slow swell",
        code: "$saw('c3').gain($sine('0.25hz').range(0, 5)).out()",
    }],
};

#[module(name = "$gain", docs = GAIN_DOCS, args(input, level), block)]
pub struct Gain {
    outputs: GainOutputs,
    params: GainParams,
    channel_state: Box<[SwapGlide]>,
}

/// The amplitude is `(level / 5)` cubed; the DSL's `GAIN_CURVE_EXP` and the
/// VU meter's dB scale assume this exponent.
struct AudioTaper;

impl Combine for AudioTaper {
    #[inline(always)]
    fn apply<T: Sample>(input: T, level: T) -> T {
        let g = level / T::from_f32(5.0);
        input * (g * g * g)
    }
}

impl Gain {
    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();
        glided_operand::update::<AudioTaper>(
            &self.params.input,
            &self.params.level,
            &mut self.channel_state,
            &mut self.outputs.sample,
            channels,
            sample_rate,
        );
    }

    fn process_block(
        &mut self,
        outputs: &mut GainBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        glided_operand::process_block::<AudioTaper>(
            &self.params.input,
            &self.params.level,
            &mut self.channel_state,
            &mut outputs.sample,
            range,
            channels,
        )
    }
}

message_handlers!(impl Gain {});
