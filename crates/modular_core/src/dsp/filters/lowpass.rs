use crate::dsp::filters::svf_module::svf_filter;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

const LPF_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonant lowpass filter",
    description: "Removes the highs above the cutoff: the everyday filter for taming bright \
                  oscillators and for envelope-swept basses and pads. Resonance adds a ringing \
                  peak at the cutoff but never self-oscillates; reach for `$lpfLadder` when \
                  you want the filter to sing.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Plucky acid bass",
            code: r#"
                const seq = $`c2 c2 [eb2 c3] g1`
                melody: $saw(seq)
                  .lpf($perc(seq, { decay: 0.2 }).range('c3', 'c7'), 4)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Slow pad sweep",
            code: r#"
                chord: $saw('c3:m7')
                  .lpf($sine('0.05hz').range('c4', 'c7'), 1)
                  .gain(4.5)
            "#,
        },
    ],
};

svf_filter! {
    docs = LPF_DOCS,
    name = "$lpf",
    ident = LowpassFilter,
    response = Lowpass,
    freq = cutoff: "cutoff frequency; tops out just under half the sample rate", default = 0.0,
    resonance = 0.0: "resonant peak at the cutoff; keeps sharpening past 5",
    db12 = "2-pole lowpass: 12 dB/oct",
    db24 = "4-pole lowpass: 24 dB/oct, flat up to the cutoff at resonance 0",
}
