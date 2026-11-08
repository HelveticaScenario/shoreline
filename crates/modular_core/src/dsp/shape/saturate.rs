//! `$unstable.shape.saturate` — Surge XT's Saturator waveshaper group.
//!
//! Ported from sst-waveshapers (GPL-3.0-or-later).

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::shape::shape_core::Shaper;
use crate::dsp::shape::shapers;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Saturation algorithm.
#[derive(Clone, Copy, Debug, Default, Deserr, JsonSchema, PartialEq, Eq, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum SaturateMode {
    /// smooth, warm saturation
    #[default]
    Soft,
    /// hard clipping, aggressive and squared-off
    Hard,
    /// lopsided saturation with a warmer, tube-like colour
    Asymmetric,
    /// between soft and hard
    Medium,
    /// pedal-style overdrive that rounds off one side early and clips the other hard
    Ojd,
}

/// Stateless — the saturator modes carry no history.
#[derive(Clone, Copy, Default)]
pub struct SaturateShaper;

impl Shaper for SaturateShaper {
    type Mode = SaturateMode;

    #[inline]
    fn shape(&mut self, x: f32, drive: f32, mode: SaturateMode, _dc: f32) -> f32 {
        match mode {
            SaturateMode::Soft => shapers::tanh(x, drive),
            SaturateMode::Hard => shapers::clip(x, drive),
            SaturateMode::Asymmetric => shapers::asym(x, drive),
            SaturateMode::Medium => shapers::zamsat(x, drive),
            SaturateMode::Ojd => shapers::ojd(x, drive),
        }
    }
}

const SATURATE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Saturation and clipping",
    description: "Warms, thickens or squares off a signal, from gentle rounding \
                  to hard clipping. Modelled on the Surge XT saturator shapes.",
    categories: &[ModuleCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "Driven bass",
        code: r#"
            $saw('c2')
              .$.unstable.shape.saturate('soft', 3)
              .$.lpf('c5')
              .out()
        "#,
    }],
};

shape_module! {
    docs = SATURATE_DOCS,
    name = "$unstable.shape.saturate", ident = Saturate, mode = SaturateMode, shaper = SaturateShaper
}
