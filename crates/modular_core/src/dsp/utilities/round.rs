use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input;
use crate::dsp::utils::Sample;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};

/// How `_round` rounds its input to a whole number.
#[derive(Clone, Copy, Debug, Default, Deserr, JsonSchema, PartialEq, Eq, Connect)]
#[serde(rename_all = "snake_case")]
#[deserr(rename_all = lowercase)]
pub enum RoundMode {
    /// to the nearest whole number, halves away from zero
    #[default]
    Round,
    /// down, towards negative infinity
    Floor,
    /// up, towards positive infinity
    Ceil,
}

impl RoundMode {
    #[inline(always)]
    fn apply<T: Sample>(self, x: T) -> T {
        match self {
            Self::Round => x.round(),
            Self::Floor => x.floor(),
            Self::Ceil => x.ceil(),
        }
    }
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct RoundParams {
    #[input("signal to round", type = audio)]
    input: PolySignal,
    #[input("which way to round to a whole volt", default = "round")]
    #[serde(default)]
    #[deserr(default)]
    mode: RoundMode,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct RoundOutputs {
    #[output("output", "the signal in whole volts", type = cv, precision = inherit, default)]
    sample: PolyOutput,
}

const ROUND_DOCS: ModuleDocs = ModuleDocs {
    summary: "Round to whole volts",
    description: "Rounds a signal to whole volts, which on a pitch means whole \
                  octaves. Backs the `.round()`, `.floor()` and `.ceil()` signal \
                  methods.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Octave-stepping arpeggio",
        code: "$sine($saw('0.5hz').range(-1, 2).round()).out()",
    }],
};

#[module(name = "_round", docs = ROUND_DOCS, args(input), block)]
pub struct Round {
    outputs: RoundOutputs,
    params: RoundParams,
}

impl Round {
    fn update(&mut self, _sample_rate: f32) {
        let mode = self.params.mode;
        if self.outputs.sample.is_f64() {
            for i in 0..self.channel_count() {
                let value = mode.apply(self.params.input.get_value_f64(i));
                self.outputs.sample.set_f64(i, value);
            }
            return;
        }
        for i in 0..self.channel_count() {
            let value = mode.apply(self.params.input.get_value(i));
            self.outputs.sample.set(i, value);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut RoundBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let mut scratch = Default::default();
        let inputs = self
            .params
            .block_inputs(range.clone(), self.channel_count(), &mut scratch)?;
        let out = &mut outputs.sample;
        let inputs = [inputs.input];
        match self.params.mode {
            RoundMode::Round => block_input::map(out, range, inputs, |[x]| x.round()),
            RoundMode::Floor => block_input::map(out, range, inputs, |[x]| x.floor()),
            RoundMode::Ceil => block_input::map(out, range, inputs, |[x]| x.ceil()),
        }
        Some(())
    }
}

message_handlers!(impl Round {});
