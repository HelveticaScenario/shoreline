//! The arithmetic operator modules: `$add`, `$sub`, `$mul`, `$div` and `$exp`.

use super::operator::operator_module;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

const ADD_DOCS: ModuleDocs = ModuleDocs {
    summary: "Add",
    description: "Adds another signal, or a fixed amount, to a signal, sample by sample.",
    categories: &[ReferenceCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Vibrato around a note",
        code: "keys: $sine($signal('a3').add($sine('5hz').amp(0.1)))",
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
    categories: &[ReferenceCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "An octave down",
        code: "keys: $saw($signal('c3').sub(1))",
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
    categories: &[ReferenceCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Ring modulation",
        code: "keys: $sine('c3').mul($sine('c4'))",
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
    categories: &[ReferenceCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "An LFO at half depth",
        code: "bass: $saw('c2').lpf($sine('0.5hz').div(2).add('c4'))",
    }],
};

operator_module!(
    Div,
    name = "$div",
    docs = DIV_DOCS,
    input = "signal to divide",
    operand = divisor: "signal or amount to divide by",
        control_range = (1.0, 16.0),
    output = "the quotient",
    // An edit glides the reciprocal, so a divisor stepping across 0 passes
    // through silence instead of through huge gains.
    glide = (
        |divisor| {
            let zero = T::from_f32(0.0);
            if divisor == zero { zero } else { T::from_f32(1.0) / divisor }
        },
        |input, reciprocal| input * reciprocal
    ),
    // Only a fixed divisor bounds the quotient: one that swings through 0
    // reaches every value.
    operand_bounds = constant,
    |input, divisor| {
        let zero = T::from_f32(0.0);
        if divisor == zero { zero } else { input / divisor }
    },
);

const EXP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Raise to a power",
    description: "Raises a signal to a power, sample by sample, with negative values \
                  mirroring positive ones, so the output only ever rises as the input \
                  does. For a curve that keeps ±5 V at ±5 V, use `$curve`.",
    categories: &[ReferenceCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "A fade-in that starts slow",
        code: "keys: $sine('c3').mul($saw('0.5hz').range(0, 1).exp(3))",
    }],
};

operator_module!(
    Exp,
    name = "$exp",
    docs = EXP_DOCS,
    input = "signal to raise",
    operand = exponent: "power to raise to",
        range = (0.0, ..),
        control_range = (0.0, 4.0),
    output = "the signal raised to the power",
    // A power too large for a sample holds at the largest one, so the output
    // and its bounds stay finite and keep rising with the input.
    |input, exponent| {
        let zero = T::from_f32(0.0);
        let largest = T::from_f32(f32::MAX);
        let exponent = if exponent < zero { zero } else { exponent };
        let magnitude = input.abs().powf(exponent);
        let magnitude = if magnitude > largest { largest } else { magnitude };
        if input == zero { zero } else { input.signum() * magnitude }
    },
);
