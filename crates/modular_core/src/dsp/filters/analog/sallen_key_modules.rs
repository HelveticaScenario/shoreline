use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

use super::module_macro::analog_kind;
use super::sallen_key::SallenKey;
use crate::dsp::filters::resonant_svf::Response;

/// One Sallen-Key filter: [`analog_kind`]'s kind, docs, name, type and
/// response.
macro_rules! sallen_key_filter {
    (
        [$($kind:tt)*]
        docs = $docs:path,
        name = $name:literal,
        ident = $ident:ident,
        response = $response:ident $(,)?
    ) => {
        analog_kind! {
            $($kind)*
            docs = $docs,
            name = $name,
            ident = $ident,
            kernel = SallenKey,
            response = Response::$response,
            options = |params| params
                .resonance
                .as_ref()
                .is_none_or(|resonance| resonance.is_constant()),
        }
    };
}

const LPF_SK_DOCS: ModuleDocs = ModuleDocs {
    summary: "Sallen-Key lowpass filter",
    description: "A snarling lowpass in the style of the Korg MS-20: resonance feeds back \
                  through a clipper, so it screams when pushed and a loud input chokes it. \
                  Turned up, it self-oscillates.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Screaming acid line",
            code: r#"
                const seq = $`c2 c2 [eb2 c3] g1`
                melody: $saw(seq).lpfSk(
                  $perc(seq, { decay: 0.25 }).range('c3', 'c7'),
                  4.2,
                )
            "#,
        },
        DocExample {
            title: "Steeper, rounder bass",
            code: "bass: $pulse('c2').lpfSk('c5', 2)._24db",
        },
    ],
};

sallen_key_filter! {
    [lowpass, two,]
    docs = LPF_SK_DOCS,
    name = "$lpfSk",
    ident = LpfSk,
    response = Lowpass,
}

const HPF_SK_DOCS: ModuleDocs = ModuleDocs {
    summary: "Sallen-Key highpass filter",
    description: "A resonant highpass in the style of the Korg MS-20: resonance feeds back \
                  through a clipper, so it squeals at the cutoff when pushed. Patch it before \
                  `$lpfSk` for the classic pair.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Thin, squealing lead",
            code: "lead: $saw('c3').hpfSk($sine('0.2hz').range('c4', 'c7'), 4)",
        },
        DocExample {
            title: "Highpass into lowpass",
            code: "bass: $saw('c2').hpfSk('c3', 3).lpfSk('c6', 3.5)",
        },
    ],
};

sallen_key_filter! {
    [highpass, two,]
    docs = HPF_SK_DOCS,
    name = "$hpfSk",
    ident = HpfSk,
    response = Highpass,
}

const BPF_SK_DOCS: ModuleDocs = ModuleDocs {
    summary: "Sallen-Key bandpass filter",
    description: "A bandpass built on the Sallen-Key core: resonance narrows the band and feeds \
                  back through a clipper, so it rings hard and self-oscillates at the centre.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Vocal sweep",
            code: r#"
                bass: $saw('c2').bpfSk(
                  $sine('0.5hz').range('c4', 'c6'),
                  3.5,
                )
            "#,
        },
        DocExample {
            title: "Pinged noise band",
            code: "noise: $noise('white').bpfSk('c6', 4.3)._24db",
        },
    ],
};

sallen_key_filter! {
    [bandpass, db12,]
    docs = BPF_SK_DOCS,
    name = "$bpfSk",
    ident = BpfSk,
    response = Bandpass,
}

const NOTCH_SK_DOCS: ModuleDocs = ModuleDocs {
    summary: "Sallen-Key notch filter",
    description: "A notch built on the Sallen-Key core: it cuts a band at the centre, and \
                  resonance narrows the cut.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Hollow sweep with bite",
            code: r#"
                bass: $saw('c2').notchSk(
                  $sine('0.3hz').range('c4', 'c7'),
                  3,
                )
            "#,
        },
        DocExample {
            title: "Two nulls through noise",
            code: "noise: $noise('white').notchSk('c6', 2)._24db",
        },
    ],
};

sallen_key_filter! {
    [notch, db12, "narrows the cut; self-oscillates from about 4.5",]
    docs = NOTCH_SK_DOCS,
    name = "$notchSk",
    ident = NotchSk,
    response = Notch,
}
