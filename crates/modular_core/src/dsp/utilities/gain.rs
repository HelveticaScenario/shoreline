use super::operator::operator_module;
use crate::dsp::utils::audio_taper;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

const GAIN_DOCS: ModuleDocs = ModuleDocs {
    summary: "Perceptual gain",
    description: "Scales a signal on a decibel curve: 5 is unity and each step of 1 is \
                  12 dB, so 4.5 is -6 dB and 4 is -12 dB. Near 0 it fades to silence. \
                  Outs apply their gain through it.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Slow swell",
        code: "keys: $saw('c3').gain($sine('0.25hz').range(0, 5))",
    }],
};

operator_module!(
    Gain,
    name = "$gain",
    docs = GAIN_DOCS,
    input = "signal to turn up or down",
    operand = level: "level: 5 unity, 12 dB per step of 1, 0 silence",
        default = 5.0,
        control_range = (0.0, 5.0),
    output = "the scaled signal",
    |input, level| input * audio_taper(level),
);
