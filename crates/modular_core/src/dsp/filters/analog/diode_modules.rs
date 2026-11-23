use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

use super::diode::Diode;
use super::module_macro::analog_kind;
use crate::dsp::filters::resonant_svf::Response;

const LPF_DIODE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Diode ladder lowpass filter",
    description: "The TB-303-style lowpass: its stages load each other, so the slope is soft and \
                  starts well below the cutoff, and the resonance is rubbery and squelching.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Acid bass",
            code: r#"
                const seq = $`c2 c2 c3 [c2 eb2] c2 g2 c2 [bb1 c3]`
                bass: $saw(seq).lpfDiode(
                  $perc(seq, { decay: 0.15 }).range('c4', 'c7'),
                  4.2,
                )
            "#,
        },
        DocExample {
            title: "Slow squelch",
            code: r#"
                bass: $pulse('c2').lpfDiode(
                  $sine('0.1hz').range('c3', 'c6'),
                  3.8,
                )
            "#,
        },
    ],
};

analog_kind! {
    lowpass,
    four,
    docs = LPF_DIODE_DOCS,
    name = "$lpfDiode",
    ident = LpfDiode,
    kernel = Diode,
    response = Response::Lowpass,
}

const HPF_DIODE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Diode ladder highpass filter",
    description: "A highpass mixed from the stages of a diode ladder, keeping its rubbery \
                  resonance at the cutoff.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Squelchy hats from noise",
            code: r#"
                const seq = $`c5*8`
                hats: $noise('white')
                  .hpfDiode('c7', 4)
                  .amp($perc(seq, { decay: 0.04 }))
            "#,
        },
        DocExample {
            title: "Thinned saw",
            code: "lead: $saw('c3').hpfDiode('c5', 3)._12db",
        },
    ],
};

analog_kind! {
    highpass,
    four,
    docs = HPF_DIODE_DOCS,
    name = "$hpfDiode",
    ident = HpfDiode,
    kernel = Diode,
    response = Response::Highpass,
}

const BPF_DIODE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Diode ladder bandpass filter",
    description: "A bandpass mixed from the stages of a diode ladder, with its rubbery resonance \
                  at the centre.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Squelching sweep",
            code: r#"
                bass: $saw('c2').bpfDiode(
                  $sine('0.3hz').range('c4', 'c7'),
                  4,
                )
            "#,
        },
        DocExample {
            title: "Nasal pulse",
            code: "lead: $pulse('c3').bpfDiode('c6', 3)._12db",
        },
    ],
};

analog_kind! {
    bandpass,
    db24,
    docs = BPF_DIODE_DOCS,
    name = "$bpfDiode",
    ident = BpfDiode,
    kernel = Diode,
    response = Response::Bandpass,
}

const NOTCH_DIODE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Diode ladder notch filter",
    description: "A notch mixed from the stages of a diode ladder. Resonance brings its rubbery \
                  character to the cut and thins the lows.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Squelching hollow",
            code: r#"
                bass: $saw('c2').notchDiode(
                  $sine('0.25hz').range('c4', 'c7'),
                  3.5,
                )
            "#,
        },
        DocExample {
            title: "Single null",
            code: "lead: $saw('c3').notchDiode('c6', 2)._12db",
        },
    ],
};

analog_kind! {
    notch,
    db24,
    docs = NOTCH_DIODE_DOCS,
    name = "$notchDiode",
    ident = NotchDiode,
    kernel = Diode,
    response = Response::Notch,
}
