//! `$unstable.shape.sine` — Surge XT's sine effect waveshaper (non-folding).
//!
//! Ported from sst-waveshapers (GPL-3.0-or-later).

use crate::dsp::shape::shape_core::Shaper;
use crate::dsp::shape::shapers;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

/// Stateless — the sine shaper carries no history.
#[derive(Clone, Copy, Default)]
pub struct SineShaper;

impl Shaper for SineShaper {
    type Mode = ();

    #[inline]
    fn shape(&mut self, x: f32, drive: f32, _mode: (), _dc: f32) -> f32 {
        shapers::sine_shaper(x, drive, false)
    }
}

const SINE_SHAPE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Sine shaper",
    description: "Bends the signal through a sine curve for smooth added \
                  harmonics; pushed harder, a pure tone folds into a hollow, \
                  bell-like one.",
    categories: &[ReferenceCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "Sweeping sine harmonics",
        code: r#"
            keys: $sine('c3').unstable.shape.sine(
              $sine('0.2hz').range(0, 4),
            )
        "#,
    }],
};

shape_module! {
    docs = SINE_SHAPE_DOCS,
    name = "$unstable.shape.sine", ident = Sine, shaper = SineShaper,
    drive_doc = "how hard the signal hits the shaper: -5 = -12 dB, 0 unity, +5 = +12 dB",
}
