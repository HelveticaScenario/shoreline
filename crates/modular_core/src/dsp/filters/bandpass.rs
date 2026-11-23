use crate::dsp::filters::svf_module::svf_filter;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

const BPF_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonant bandpass filter",
    description: "Passes a band around the centre frequency and cuts both sides. Narrow it \
                  with resonance for vocal, wah-like tones, or sweep it over noise for wind \
                  and whooshes.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Auto-wah saw",
            code: "bass: $saw('c2').bpf($sine('2hz').range('c4', 'c6'), 3)",
        },
        DocExample {
            title: "Whistling wind",
            code: r#"
                noise: $noise('white').bpf(
                  $lorenz({ rate: 0.1 }).range('c5', 'c7'),
                  4.5,
                )
            "#,
        },
    ],
};

svf_filter! {
    docs = BPF_DOCS,
    name = "$bpf",
    ident = BandpassFilter,
    response = Bandpass,
    freq = center: "centre frequency; tops out just under half the sample rate",
    resonance = 1.0: "higher narrows the band, and keeps narrowing past 5",
    db12 = "2-pole bandpass: 6 dB/oct each side",
    db24 = "4-pole bandpass: 12 dB/oct each side, flat across the band",
}
