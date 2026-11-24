use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

use super::cascade::{Cascade, LadderStages};
use super::kernel::LoopResponse;
use super::module_macro::analog_kind;
use crate::dsp::filters::resonant_svf::Response;

const LPF_LADDER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Transistor ladder lowpass filter",
    description: "The Moog-style four-pole lowpass: every stage saturates, so it thickens what \
                  passes through, and the bass thins as resonance rises. It self-oscillates into \
                  a sine at the cutoff.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Fat swept bass",
            code: r#"
                const seq = $`c1 c2 c1 [eb2 g1]`
                bass: $saw(seq).lpfLadder(
                  $perc(seq, { decay: 0.3 }).range('c3', 'c6'),
                  3,
                )
            "#,
        },
        DocExample {
            title: "Self-oscillating sine at A4",
            code: "sine: $lpfLadder(0, 'a4', 5)",
        },
    ],
};

analog_kind! {
    lowpass,
    four,
    docs = LPF_LADDER_DOCS,
    name = "$lpfLadder",
    ident = LpfLadder,
    kernel = Cascade<LadderStages>,
    response = LoopResponse::Base(Response::Lowpass),
}

const HPF_LADDER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Transistor ladder highpass filter",
    description: "A highpass mixed from the stages of a Moog-style ladder: the lows fall away \
                  while the ladder's resonance and saturation stay.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Resonant riser",
            code: r#"
                noise: $noise('white').hpfLadder(
                  $clock._ramp.range('c3', 'c8'),
                  3.5,
                )
            "#,
        },
        DocExample {
            title: "Gentler slope",
            code: "chord: $saw('c3:m7').hpfLadder('c5', 2)._12db.gain(4.5)",
        },
    ],
};

analog_kind! {
    highpass,
    four,
    docs = HPF_LADDER_DOCS,
    name = "$hpfLadder",
    ident = HpfLadder,
    kernel = Cascade<LadderStages>,
    response = LoopResponse::Base(Response::Highpass),
}

const BPF_LADDER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Transistor ladder bandpass filter",
    description: "A bandpass mixed from the stages of a Moog-style ladder, with the ladder's \
                  resonance and saturation at the centre.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Wah bass",
            code: r#"
                bass: $saw('c2').bpfLadder(
                  $sine('1hz').range('c4', 'c6'),
                  3,
                )
            "#,
        },
        DocExample {
            title: "Wide band on a pad",
            code: "chord: $saw('c3:M7').bpfLadder('c5', 1)._12db.gain(4.5)",
        },
    ],
};

analog_kind! {
    bandpass,
    db24,
    docs = BPF_LADDER_DOCS,
    name = "$bpfLadder",
    ident = BpfLadder,
    kernel = Cascade<LadderStages>,
    response = LoopResponse::Base(Response::Bandpass),
}

const NOTCH_LADDER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Transistor ladder notch filter",
    description: "A notch mixed from the stages of a Moog-style ladder. Resonance sharpens the \
                  filter around the cut and thins the lows, and the cut fills in as the ladder \
                  saturates.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Twin nulls sweeping a pad",
            code: r#"
                chord: $saw('c3:m7')
                  .notchLadder($sine('0.2hz').range('c4', 'c7'), 2)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Single null",
            code: "lead: $pulse('c3').notchLadder('c6', 1)._12db",
        },
    ],
};

analog_kind! {
    notch,
    db24,
    docs = NOTCH_LADDER_DOCS,
    name = "$notchLadder",
    ident = NotchLadder,
    kernel = Cascade<LadderStages>,
    response = LoopResponse::Base(Response::Notch),
}

const APF_LADDER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Transistor ladder allpass filter",
    description: "An allpass mixed from the stages of a Moog-style ladder: it shifts phase \
                  around the cutoff, so mixing it with the dry signal makes a phaser. Resonance \
                  sharpens the sweep.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Phaser",
            code: r#"
                chord: $saw('c3:m7')
                  .apfLadder($sine('0.3hz').range('c4', 'c7'), 2)
                  .wet(2.5)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "One-notch phaser",
            code: r#"
                chord: $saw('c3:M7')
                  .apfLadder($sine('0.2hz').range('c5', 'c7'))
                  ._12db.wet(2.5)
            "#,
        },
    ],
};

analog_kind! {
    allpass,
    docs = APF_LADDER_DOCS,
    name = "$apfLadder",
    ident = ApfLadder,
    kernel = Cascade<LadderStages>,
    response = LoopResponse::Allpass,
}
