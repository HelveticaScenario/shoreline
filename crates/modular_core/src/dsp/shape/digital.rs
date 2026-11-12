//! `$unstable.shape.digital` — Surge XT's digital (sample-quantizing) effect waveshaper.
//!
//! Ported from sst-waveshapers (GPL-3.0-or-later).

use crate::dsp::shape::shape_core::Shaper;
use crate::dsp::shape::shapers;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Stateless — quantization carries no history.
#[derive(Clone, Copy, Default)]
pub struct DigitalShaper;

impl Shaper for DigitalShaper {
    type Mode = ();

    #[inline]
    fn shape(&mut self, x: f32, drive: f32, _mode: (), _dc: f32) -> f32 {
        shapers::digital(x, drive)
    }
}

const DIGITAL_DOCS: ModuleDocs = ModuleDocs {
    summary: "Bit-crusher",
    description: "Steps the signal down to coarse levels for a lo-fi, digital \
                  crunch; raise `drive` for coarser steps.",
    categories: &[ModuleCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "Crunchy chord",
        code: "$saw($chord('C3[m7]')).unstable.shape.digital(2).out()",
    }],
};

shape_module! {
    docs = DIGITAL_DOCS,
    name = "$unstable.shape.digital", ident = Digital, shaper = DigitalShaper,
    drive_doc = "step size: -5 fine, about 128 levels across ±5 V; 0 about 32; +5 coarse, about 8",
}
