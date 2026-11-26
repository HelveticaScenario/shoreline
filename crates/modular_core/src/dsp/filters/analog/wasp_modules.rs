use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

use super::module_macro::analog_filter;
use super::wasp::{Wasp, WaspResponse};

/// One Wasp filter: its docs, name, type and response, its frequency
/// param, `cutoff` or `center`, and what its resonance and output are. A
/// resonance of `peak` is the lowpass's and highpass's.
macro_rules! wasp_filter {
    (
        docs = $docs:path,
        name = $name:literal,
        ident = $ident:ident,
        response = $response:ident,
        freq = cutoff,
        $($rest:tt)+
    ) => {
        wasp_filter! {
            docs = $docs,
            name = $name,
            ident = $ident,
            response = $response,
            freq = cutoff: "cutoff frequency; tops out just under half the sample rate",
            $($rest)+
        }
    };
    (
        docs = $docs:path,
        name = $name:literal,
        ident = $ident:ident,
        response = $response:ident,
        freq = center,
        $($rest:tt)+
    ) => {
        wasp_filter! {
            docs = $docs,
            name = $name,
            ident = $ident,
            response = $response,
            freq = center: "centre frequency; tops out just under half the sample rate",
            $($rest)+
        }
    };
    (
        docs = $docs:path,
        name = $name:literal,
        ident = $ident:ident,
        response = $response:ident,
        freq = $freq:ident: $freq_doc:literal,
        resonance = peak,
        $($rest:tt)+
    ) => {
        wasp_filter! {
            docs = $docs,
            name = $name,
            ident = $ident,
            response = $response,
            freq = $freq: $freq_doc,
            resonance = "resonant peak at the cutoff, most of it past 4 and less of it at high cutoffs; whistles at 5 without self-oscillating",
            $($rest)+
        }
    };
    (
        docs = $docs:path,
        name = $name:literal,
        ident = $ident:ident,
        response = $response:ident,
        freq = $freq:ident: $freq_doc:literal,
        resonance = $resonance_doc:literal,
        output = $output_doc:literal $(,)?
    ) => {
        analog_filter! {
            docs = $docs,
            name = $name,
            ident = $ident,
            kernel = Wasp,
            response = WaspResponse::$response,
            drive = [],
            freq = $freq,
            freq_doc = $freq_doc,
            resonance_doc = $resonance_doc,
            outputs = {
                db12[crate::dsp::filters::analog::kernel::DB12] = "12db": $output_doc, default;
            },
        }
    };
}

const LPF_WASP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Wasp lowpass filter",
    description: "The lowpass of the EDP Wasp's filter as Doepfer's A-124 builds it: \
                  logic-chip inverters standing in for amplifiers. A loud input pushes the inverters toward their \
                  rails, which dirties the sound and makes the resonance erratic, so turn the \
                  input down for a cleaner tone. It never self-oscillates, and like the \
                  hardware it passes no DC.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Buzzing bass",
            code: r#"
                const seq = $`c2 c2 [eb2 c3] g1`
                bass: $saw(seq).lpfWasp(
                  $perc(seq, { decay: 0.25 }).range('c3', 'c7'),
                  4,
                )
            "#,
        },
        DocExample {
            title: "Whistling lead",
            code: "lead: $pulse('c3').lpfWasp('c6', 4.6)",
        },
    ],
};

wasp_filter! {
    docs = LPF_WASP_DOCS,
    name = "$lpfWasp",
    ident = LpfWasp,
    response = Lowpass,
    freq = cutoff,
    resonance = peak,
    output = "2-pole lowpass: 12 dB/oct",
}

const HPF_WASP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Wasp highpass filter",
    description: "The highpass of the EDP Wasp's inverter filter: thin and wiry, with a \
                  resonance that whistles at the cutoff without self-oscillating and grit \
                  from a loud input.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Wiry lead",
            code: r#"
                lead: $saw('c3').hpfWasp(
                  $sine('0.2hz').range('c4', 'c7'),
                  4,
                )
            "#,
        },
        DocExample {
            title: "Pad thinning out and back",
            code: r#"
                chord: $saw('c3:M7')
                  .hpfWasp($sine('0.1hz').range('c3', 'c6'), 2)
                  .gain(4.5)
            "#,
        },
    ],
};

wasp_filter! {
    docs = HPF_WASP_DOCS,
    name = "$hpfWasp",
    ident = HpfWasp,
    response = Highpass,
    freq = cutoff,
    resonance = peak,
    output = "2-pole highpass: 12 dB/oct",
}

const BPF_WASP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Wasp bandpass filter",
    description: "The bandpass of the EDP Wasp's inverter filter. Resonance narrows the band \
                  and raises its level, up to a nasal whistle that squashes as it gets loud \
                  and never self-oscillates.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Nasal wah",
            code: r#"
                bass: $saw('c2').bpfWasp(
                  $sine('2hz').range('c4', 'c6'),
                  3.5,
                )
            "#,
        },
        DocExample {
            title: "Whistling wind",
            code: r#"
                noise: $noise('white').bpfWasp(
                  $lorenz({ rate: 0.1 }).range('c5', 'c7'),
                  5,
                )
            "#,
        },
    ],
};

wasp_filter! {
    docs = BPF_WASP_DOCS,
    name = "$bpfWasp",
    ident = BpfWasp,
    response = Bandpass,
    freq = center,
    resonance = "narrows the band and raises its level, most of all past 4; whistles at 5 without self-oscillating",
    output = "2-pole bandpass: 6 dB/oct each side",
}

const NOTCH_WASP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Wasp notch filter",
    description: "The highpass and lowpass of the EDP Wasp's inverter filter mixed together, \
                  which cancel at the centre. Resonance narrows the cut, and near the top \
                  the filter's grit fills it back in.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Hollow sweep through a pad",
            code: r#"
                chord: $saw('c3:m7')
                  .notchWasp($sine('0.2hz').range('c4', 'c7'), 2)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Narrow cut through noise",
            code: r#"
                noise: $noise('white').notchWasp(
                  $sine('0.1hz').range('c5', 'c8'),
                  3.5,
                )
            "#,
        },
    ],
};

wasp_filter! {
    docs = NOTCH_WASP_DOCS,
    name = "$notchWasp",
    ident = NotchWasp,
    response = Notch,
    freq = center,
    resonance = "narrows the cut; it fills back in toward 5",
    output = "one null at the centre",
}

const APF_WASP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Wasp allpass filter",
    description: "An allpass mixed from the three outputs of the EDP Wasp's inverter \
                  filter: it shifts phase around the cutoff and keeps the level, so mixing it \
                  with the dry signal makes a phaser with the filter's grit on a loud input. Near \
                  full resonance the level strays at the cutoff.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Phaser",
            code: r#"
                chord: $saw('c3:M9')
                  .apfWasp($sine('0.3hz').range('c4', 'c7'), 2)
                  .wet(2.5)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Sharper sweep",
            code: r#"
                lead: $pulse('c3')
                  .apfWasp($sine('0.2hz').range('c5', 'c7'), 4)
                  .wet(2.5)
            "#,
        },
    ],
};

wasp_filter! {
    docs = APF_WASP_DOCS,
    name = "$apfWasp",
    ident = ApfWasp,
    response = Allpass,
    freq = cutoff,
    resonance = "sharpens the phase sweep; toward 5 the level strays at the cutoff",
    output = "360° of phase shift across the cutoff: one notch when mixed with the dry signal",
}
