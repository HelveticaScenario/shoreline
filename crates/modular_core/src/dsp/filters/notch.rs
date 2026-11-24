use crate::dsp::filters::svf_module::svf_filter;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

const NOTCH_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonant notch filter",
    description: "Cuts a narrow band at the centre frequency and passes the rest. Sweep it \
                  for a phaser-like hollow moving through the sound, or park it to remove a \
                  hum or a harsh partial.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Hollow sweep through a pad",
            code: r#"
                chord: $saw('c3:m7')
                  .notch($sine('0.2hz').range('c4', 'c7'), 1)
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Two nulls sweeping noise",
            code: r#"
                noise: $notch(
                  $noise('white'),
                  $sine('0.1hz').range('c4', 'c8'),
                  2,
                )._24db
            "#,
        },
    ],
};

svf_filter! {
    docs = NOTCH_DOCS,
    name = "$notch",
    ident = NotchFilter,
    response = Notch,
    freq = center: "centre frequency; tops out just under half the sample rate",
    resonance = 1.0: "higher narrows the cut, and keeps narrowing past 5",
    db12 = "one null at the centre",
    db24 = "two nulls, half an octave either side of the centre",
}
