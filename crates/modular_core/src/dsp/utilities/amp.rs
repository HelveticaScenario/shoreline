use super::operator::operator_module;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

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

operator_module!(
    Amp,
    name = "$amp",
    docs = AMP_DOCS,
    input = "signal to turn up or down",
    operand = factor: "linear gain: 5 unity, 0 silence, 10 double, negative inverts",
        default = 5.0,
        control_range = (0.0, 10.0),
    output = "the scaled signal",
    |input, factor| input * (factor / T::from_f32(5.0)),
);
