use crate::dsp::filters::svf_module::svf_filter;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

const HPF_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonant highpass filter",
    description: "Removes the lows below the cutoff: clears rumble, thins a part so it sits \
                  in a mix, or builds a rising sweep. Resonance adds a ringing peak at the cutoff.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Pad thinning out and back",
            code: r#"
                chord: $saw('c3:M7').hpf(
                  $sine('0.1hz').range('c3', 'c6'),
                  2,
                )
            "#,
        },
        DocExample {
            title: "Riser across each bar",
            code: r#"
                noise: $noise('white').hpf(
                  $clock._ramp.range('c2', 'c9'),
                  2,
                )
            "#,
        },
    ],
};

svf_filter! {
    docs = HPF_DOCS,
    name = "$hpf",
    ident = HighpassFilter,
    response = Highpass,
    freq = cutoff: "cutoff frequency; tops out just under half the sample rate",
    resonance = 0.0: "resonant peak at the cutoff; keeps sharpening past 5",
    db12 = "2-pole highpass: 12 dB/oct",
    db24 = "4-pole highpass: 24 dB/oct, flat above the cutoff at resonance 0",
}
