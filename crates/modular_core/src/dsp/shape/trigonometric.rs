//! `$unstable.shape.trigonometric` — Surge XT's Trigonometric waveshaper group.
//!
//! Ported from sst-waveshapers (GPL-3.0-or-later).

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::shape::shape_core::Shaper;
use crate::dsp::shape::shapers;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Trigonometric shaping algorithm.
#[derive(Clone, Copy, Debug, Default, Deserr, JsonSchema, PartialEq, Eq, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum TrigonometricMode {
    /// adds a single sine ripple to the signal
    #[default]
    SinPlusX,
    /// adds 2 sine ripples riding on the signal
    #[serde(rename = "sin2x")]
    #[deserr(rename = "sin2x")]
    Sin2x,
    /// adds 3 sine ripples riding on the signal
    #[serde(rename = "sin3x")]
    #[deserr(rename = "sin3x")]
    Sin3x,
    /// adds 7 sine ripples riding on the signal
    #[serde(rename = "sin7x")]
    #[deserr(rename = "sin7x")]
    Sin7x,
    /// adds 10 sine ripples riding on the signal
    #[serde(rename = "sin10x")]
    #[deserr(rename = "sin10x")]
    Sin10x,
    /// maps the signal through 2 sine cycles, dense harmonics
    Cyc2,
    /// maps the signal through 7 sine cycles, denser harmonics
    Cyc7,
    /// maps the signal through 10 sine cycles, densest harmonics
    Cyc10,
    /// 2 sine cycles, tapered toward the edges
    Cyc2Bound,
    /// 7 sine cycles, tapered toward the edges
    Cyc7Bound,
    /// 10 sine cycles, tapered toward the edges
    Cyc10Bound,
}

/// Stateless — the trigonometric modes carry no history.
#[derive(Clone, Copy, Default)]
pub struct TrigonometricShaper;

impl Shaper for TrigonometricShaper {
    type Mode = TrigonometricMode;

    #[inline]
    fn shape(&mut self, x: f32, drive: f32, mode: TrigonometricMode, _dc: f32) -> f32 {
        use TrigonometricMode::*;
        match mode {
            SinPlusX => shapers::sin_plus_x(x, drive),
            Sin2x => shapers::sin_nx_plus_x_bound(x, drive, 2.0),
            Sin3x => shapers::sin_nx_plus_x_bound(x, drive, 3.0),
            Sin7x => shapers::sin_nx_plus_x_bound(x, drive, 7.0),
            Sin10x => shapers::sin_nx_plus_x_bound(x, drive, 10.0),
            Cyc2 => shapers::sin_nx(x, drive, 2.0),
            Cyc7 => shapers::sin_nx(x, drive, 7.0),
            Cyc10 => shapers::sin_nx(x, drive, 10.0),
            Cyc2Bound => shapers::sin_nx_bound(x, drive, 2.0),
            Cyc7Bound => shapers::sin_nx_bound(x, drive, 7.0),
            Cyc10Bound => shapers::sin_nx_bound(x, drive, 10.0),
        }
    }
}

const TRIGONOMETRIC_DOCS: ModuleDocs = ModuleDocs {
    summary: "Metallic sine-ripple shaper",
    description: "Many-cycle sine shapes for dense, ringing, metallic \
                  harmonics that change sharply with level, so a slowly swept \
                  drive makes one oscillator glitter. Modelled on \
                  the Surge XT trigonometric shapes.",
    categories: &[ModuleCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "Ringing sweep",
        code: "$sine('c3').$.unstable.shape.trigonometric('cyc7', $sine('0.2hz').range(-3, 2)).out()",
    }],
};

shape_module! {
    docs = TRIGONOMETRIC_DOCS,
    name = "$unstable.shape.trigonometric", ident = Trigonometric, mode = TrigonometricMode, shaper = TrigonometricShaper
}
