use super::operator::operator_module;
use crate::dsp::utils::audio_taper;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

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

operator_module!(
    Gain,
    name = "$gain",
    docs = GAIN_DOCS,
    input = "signal to turn up or down",
    operand = level: "perceptual level: 5 unity, 0 silence, each halving about -18 dB",
        default = 5.0,
        control_range = (0.0, 5.0),
    output = "the scaled signal",
    |input, level| input * audio_taper(level),
);
