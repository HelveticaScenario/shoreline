use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use super::operator::operator_module;
use crate::block_input;
use crate::dsp::utils::Sample;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};

const ADD_DOCS: ModuleDocs = ModuleDocs {
    summary: "Add",
    description: "Adds another signal, or a fixed amount, to a signal, sample by sample.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Vibrato around a note",
        code: "$sine($signal('a3').add($sine('5hz').amp(0.1))).out()",
    }],
};

operator_module!(
    Add,
    name = "$add",
    docs = ADD_DOCS,
    input = "signal to add to",
    operand = offset: "signal or amount added",
        control_range = (-5.0, 5.0),
    output = "the sum",
    |input, offset| input + offset,
);

const SUB_DOCS: ModuleDocs = ModuleDocs {
    summary: "Subtract",
    description: "Subtracts another signal, or a fixed amount, from a signal, sample by \
                  sample.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "An octave down",
        code: "$saw($signal('c3').sub(1)).out()",
    }],
};

operator_module!(
    Sub,
    name = "$sub",
    docs = SUB_DOCS,
    input = "signal to subtract from",
    operand = offset: "signal or amount subtracted",
        control_range = (-5.0, 5.0),
    output = "the difference",
    |input, offset| input - offset,
);

const MUL_DOCS: ModuleDocs = ModuleDocs {
    summary: "Multiply",
    description: "Multiplies a signal by another signal, or a fixed amount, sample by \
                  sample: 2 doubles it. To change a level with a 0–5 V signal such as \
                  `$adsr`, use `$amp` or `$gain`.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Ring modulation",
        code: "$sine('c3').mul($sine('c4')).out()",
    }],
};

operator_module!(
    Mul,
    name = "$mul",
    docs = MUL_DOCS,
    input = "signal to multiply",
    operand = factor: "signal or amount to multiply by; negative inverts",
        control_range = (-2.0, 2.0),
    output = "the product",
    |input, factor| input * factor,
);

const DIV_DOCS: ModuleDocs = ModuleDocs {
    summary: "Divide",
    description: "Divides a signal by another signal, or a fixed amount, sample by \
                  sample. Dividing by 0 gives 0 V.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "An LFO at half depth",
        code: "$saw('c2').lpf($sine('0.5hz').div(2).add('c4')).out()",
    }],
};

operator_module!(
    Div,
    name = "$div",
    docs = DIV_DOCS,
    input = "signal to divide",
    operand = divisor: "signal or amount to divide by",
        control_range = (1.0, 10.0),
    output = "the quotient",
    |input, divisor| input / divisor,
);

const EXP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Raise to a power",
    description: "Raises a signal to a power, sample by sample, with negative values \
                  mirroring positive ones. An exponent below 0 holds at 0, so the output \
                  only ever rises as the input does. For a curve that keeps ±5 V at ±5 V, \
                  use `$curve`.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "A fade-in that starts slow",
        code: "$sine('c3').mul($saw('0.5hz').range(0, 1).exp(3)).out()",
    }],
};

operator_module!(
    Exp,
    name = "$exp",
    docs = EXP_DOCS,
    input = "signal to raise",
    operand = exponent: "power to raise to; below 0 holds at 0",
        control_range = (0.0, 4.0),
    output = "the signal raised to the power",
    |input, exponent| {
        let zero = T::from_f32(0.0);
        let exponent = if exponent < zero { zero } else { exponent };
        input.signum() * input.abs().powf(exponent)
    },
);

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
