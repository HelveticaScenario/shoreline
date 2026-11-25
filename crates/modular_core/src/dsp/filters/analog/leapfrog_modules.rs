use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

use super::kernel::DB24;
use super::leapfrog::{Leapfrog, LeapfrogResponse};
use super::module_macro::analog_filter;

const LPF_LEAPFROG_DOCS: ModuleDocs = ModuleDocs {
    summary: "Leapfrog brick-wall lowpass filter",
    description: "The North Coast Synthesis Leapfrog's five-pole near-elliptic lowpass: almost \
                  a brick wall above the cutoff, with nulls at twice and three times the cutoff \
                  and a rippling passband that rings on sharp edges. Resonance feeds the output \
                  back through a clipper until the filter sings, about a fourth below the \
                  cutoff.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Brick-wall sweep",
            code: r#"
                chord: $saw('c2:m7')
                  .lpfLeapfrog($sine('0.1hz').range('c4', 'c7'))
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Ringing bass",
            code: r#"
                const seq = $`c2 c2 [eb2 c3] g1`
                bass: $pulse(seq).lpfLeapfrog(
                  $perc(seq, { decay: 0.25 }).range('c3', 'c6'),
                  3.5,
                )
            "#,
        },
    ],
};

analog_filter! {
    docs = LPF_LEAPFROG_DOCS,
    name = "$lpfLeapfrog",
    ident = LpfLeapfrog,
    kernel = Leapfrog,
    response = LeapfrogResponse::Lowpass,
    freq = cutoff,
    freq_doc = "cutoff frequency: the last peak before the wall; tops out just under half the sample rate",
    resonance_doc = "feedback around the filter, raising a peak below the cutoff; self-oscillates from about 4.5",
    outputs = {
        output[DB24] = "output": "5-pole near-elliptic lowpass", default;
    },
}

const NOTCH_LEAPFROG_DOCS: ModuleDocs = ModuleDocs {
    summary: "Leapfrog twin-null notch filter",
    description: "A notch mixed from the integrators of the North Coast Synthesis Leapfrog: \
                  two nulls about half an octave apart, the lower one wide and the upper one \
                  narrow. Resonance sharpens both and never self-oscillates.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Twin nulls sweeping a pad",
            code: r#"
                chord: $saw('c3:m7')
                  .notchLeapfrog($sine('0.15hz').range('c4', 'c7'), 2)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Hollowed noise",
            code: r#"
                noise: $noise('white').notchLeapfrog(
                  $sine('0.3hz').range('c5', 'c8'),
                  4,
                )
            "#,
        },
    ],
};

analog_filter! {
    docs = NOTCH_LEAPFROG_DOCS,
    name = "$notchLeapfrog",
    ident = NotchLeapfrog,
    kernel = Leapfrog,
    response = LeapfrogResponse::Notch,
    freq = center,
    freq_doc = "centre frequency, between the two nulls; tops out just under half the sample rate",
    resonance_doc = "feedback around the filter, narrowing both nulls",
    outputs = {
        output[DB24] = "output": "two nulls, a quarter octave either side of the centre", default;
    },
}

const APF_LEAPFROG_DOCS: ModuleDocs = ModuleDocs {
    summary: "Leapfrog allpass filter",
    description: "An allpass mixed from the integrators of the North Coast Synthesis Leapfrog: \
                  it shifts phase around the cutoff, gently below it and abruptly above, so \
                  mixing it with the dry signal makes a phaser with one wide notch and one \
                  narrow. Resonance raises peaks where the notches would be.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Phaser",
            code: r#"
                chord: $saw('c3:M9')
                  .apfLeapfrog($sine('0.3hz').range('c4', 'c7'))
                  .wet(2.5)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Resonant phaser",
            code: r#"
                chord: $saw('c3:m7')
                  .apfLeapfrog($sine('0.2hz').range('c5', 'c7'), 3)
                  .wet(2.5)
            "#,
        },
    ],
};

analog_filter! {
    docs = APF_LEAPFROG_DOCS,
    name = "$apfLeapfrog",
    ident = ApfLeapfrog,
    kernel = Leapfrog,
    response = LeapfrogResponse::Allpass,
    freq = cutoff,
    freq_doc = "cutoff frequency, where the phase has turned 360°; tops out just under half the sample rate",
    resonance_doc = "feedback around the filter, raising a peak either side of the cutoff; self-oscillates from about 4.5",
    outputs = {
        output[DB24] = "output": "720° of phase shift across the cutoff at a constant level: two notches when mixed with the dry signal", default;
    },
}
