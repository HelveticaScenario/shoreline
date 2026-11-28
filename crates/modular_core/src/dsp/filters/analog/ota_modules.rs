use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

use super::cascade::{Cascade, OtaStages};
use super::kernel::LoopResponse;
use super::module_macro::analog_kind;
use crate::dsp::filters::resonant_svf::Response;

const LPF_OTA_DOCS: ModuleDocs = ModuleDocs {
    summary: "OTA cascade lowpass filter",
    description: "The Roland-style four-pole lowpass built from OTA stages: clean and full in \
                  the bass, since the stages only saturate when the signal moves fast. It self- \
                  oscillates into a sine at the cutoff.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Breathing pad",
            code: r#"
                chord: $saw('c3:M9')
                  .lpfOta($sine('0.1hz').range('c4', 'c7'), 2.5)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Plucked bass",
            code: r#"
                const seq = $`c2 [c2 c3] eb2 [g1 bb1]`
                bass: $pulse(seq).lpfOta(
                  $perc(seq, { decay: 0.2 }).range('c3', 'c6'),
                  3.5,
                )
            "#,
        },
    ],
};

analog_kind! {
    lowpass,
    four,
    docs = LPF_OTA_DOCS,
    name = "$lpfOta",
    ident = LpfOta,
    kernel = Cascade<OtaStages>,
    response = LoopResponse::Base(Response::Lowpass),
}

const HPF_OTA_DOCS: ModuleDocs = ModuleDocs {
    summary: "OTA cascade highpass filter",
    description: "A highpass mixed from the stages of an OTA cascade, as in the Oberheim \
                  Xpander: clean below resonance, with a singing peak at the cutoff.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Pad thinning out and back",
            code: r#"
                chord: $saw('c3:M7')
                  .hpfOta($sine('0.1hz').range('c3', 'c6'), 2)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Gentler slope",
            code: "lead: $saw('c4').hpfOta('c5', 3)._12db",
        },
    ],
};

analog_kind! {
    highpass,
    four,
    docs = HPF_OTA_DOCS,
    name = "$hpfOta",
    ident = HpfOta,
    kernel = Cascade<OtaStages>,
    response = LoopResponse::Base(Response::Highpass),
}

const BPF_OTA_DOCS: ModuleDocs = ModuleDocs {
    summary: "OTA cascade bandpass filter",
    description: "A bandpass mixed from the stages of an OTA cascade, as in the Oberheim \
                  Xpander, with a singing peak at the centre.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Auto-wah",
            code: "bass: $saw('c2').bpfOta($sine('2hz').range('c4', 'c6'), 3)",
        },
        DocExample {
            title: "Whistling wind",
            code: r#"
                noise: $noise('white').bpfOta(
                  $lorenz({ rate: 0.1 }).range('c5', 'c7'),
                  4.4,
                )
            "#,
        },
    ],
};

analog_kind! {
    bandpass,
    db24,
    docs = BPF_OTA_DOCS,
    name = "$bpfOta",
    ident = BpfOta,
    kernel = Cascade<OtaStages>,
    response = LoopResponse::Base(Response::Bandpass),
}

const NOTCH_OTA_DOCS: ModuleDocs = ModuleDocs {
    summary: "OTA cascade notch filter",
    description: "A notch mixed from the stages of an OTA cascade, as in the Oberheim Xpander: a \
                  clean cut, which resonance sharpens while thinning the lows.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Twin nulls sweeping a pad",
            code: r#"
                chord: $saw('c3:M9')
                  .notchOta($sine('0.15hz').range('c4', 'c7'), 2)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Single null",
            code: "lead: $pulse('c3').notchOta('c6', 1)._12db",
        },
    ],
};

analog_kind! {
    notch,
    db24,
    docs = NOTCH_OTA_DOCS,
    name = "$notchOta",
    ident = NotchOta,
    kernel = Cascade<OtaStages>,
    response = LoopResponse::Base(Response::Notch),
}

const APF_OTA_DOCS: ModuleDocs = ModuleDocs {
    summary: "OTA cascade allpass filter",
    description: "An allpass mixed from the stages of an OTA cascade, as in the Oberheim \
                  Xpander: it shifts phase around the cutoff, so mixing it with the dry signal \
                  makes a clean phaser. Resonance sharpens the sweep.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Phaser",
            code: r#"
                chord: $saw('c3:M9')
                  .apfOta($sine('0.3hz').range('c4', 'c7'), 2).wet(2.5)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "One-notch phaser",
            code: r#"
                chord: $saw('c3:m7')
                  .apfOta($sine('0.2hz').range('c5', 'c7'))
                  ._12db.wet(2.5)
            "#,
        },
    ],
};

analog_kind! {
    allpass,
    docs = APF_OTA_DOCS,
    name = "$apfOta",
    ident = ApfOta,
    kernel = Cascade<OtaStages>,
    response = LoopResponse::Allpass,
}
