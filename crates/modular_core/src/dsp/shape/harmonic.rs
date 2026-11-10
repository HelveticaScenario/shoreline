//! `$unstable.shape.harmonic` — Surge XT's Harmonic waveshaper group (Chebyshev + additive).
//!
//! Ported from sst-waveshapers (GPL-3.0-or-later).

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::shape::shape_core::Shaper;
use crate::dsp::shape::shapers::{self, ADDITIVE_SCALE};
use crate::dsp::utils::dc_blocker::DcBlocker;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Harmonic-generating algorithm.
#[derive(Clone, Copy, Debug, Default, Deserr, JsonSchema, PartialEq, Eq, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum HarmonicMode {
    /// adds a soft 2nd harmonic
    #[default]
    Cheby2,
    /// adds a soft 3rd harmonic
    Cheby3,
    /// adds a soft 4th harmonic
    Cheby4,
    /// adds a soft 5th harmonic
    Cheby5,
    /// blend of the 1st and 2nd harmonics
    Add12,
    /// blend of the 1st and 3rd harmonics
    Add13,
    /// blend of the 1st and 4th harmonics
    Add14,
    /// blend of the 1st and 5th harmonics
    Add15,
    /// blend of the 1st through 5th harmonics
    Add12345,
    /// sawtooth-like blend of the first three harmonics
    AddSaw3,
    /// square-like blend of odd harmonics
    AddSqr3,
}

/// `tanh(in·scale) → Chebyshev series` — the shared additive-harmonic path.
#[inline]
fn additive(x: f32, drive: f32, weights: &[f32], scale: f32) -> f32 {
    let inp = shapers::tanh_driven(x * scale * drive);
    shapers::cheb_series(inp, weights)
}

/// Holds the DC blocker the Chebyshev and even additive modes need.
#[derive(Clone, Copy, Default)]
pub struct HarmonicShaper {
    dc: DcBlocker,
}

impl Shaper for HarmonicShaper {
    type Mode = HarmonicMode;

    #[inline]
    fn shape(&mut self, x: f32, drive: f32, mode: HarmonicMode, dc: f32) -> f32 {
        use HarmonicMode::*;
        // Chebyshev: clamp, apply the polynomial, DC-block, then tanh with drive.
        let cheby = |k: fn(f32) -> f32, dcb: &mut DcBlocker| {
            shapers::tanh(dcb.process(shapers::cheby_bound(x, k), dc), drive)
        };
        match mode {
            Cheby2 => cheby(shapers::cheb2, &mut self.dc),
            Cheby3 => cheby(shapers::cheb3, &mut self.dc),
            Cheby4 => cheby(shapers::cheb4, &mut self.dc),
            Cheby5 => cheby(shapers::cheb5, &mut self.dc),
            Add12 => self
                .dc
                .process(additive(x, drive, &[0.0, 0.5, 0.5], ADDITIVE_SCALE), dc),
            Add13 => additive(x, drive, &[0.0, 0.5, 0.0, 0.5], ADDITIVE_SCALE),
            Add14 => self.dc.process(
                additive(x, drive, &[0.0, 0.5, 0.0, 0.0, 0.5], ADDITIVE_SCALE),
                dc,
            ),
            Add15 => additive(x, drive, &[0.0, 0.5, 0.0, 0.0, 0.0, 0.5], ADDITIVE_SCALE),
            Add12345 => self.dc.process(
                additive(x, drive, &[0.0, 0.2, 0.2, 0.2, 0.2, 0.2], ADDITIVE_SCALE),
                dc,
            ),
            AddSaw3 => {
                let fac = 0.9 / (1.0 + 0.5 + 0.25);
                self.dc.process(
                    additive(
                        x,
                        drive,
                        &[0.0, -fac, fac * 0.5, -fac * 0.25],
                        -ADDITIVE_SCALE,
                    ),
                    dc,
                )
            }
            AddSqr3 => {
                let fac = 0.9 / (1.0 - 0.25 + 1.0 / 16.0);
                additive(
                    x,
                    drive,
                    &[0.0, fac, 0.0, -fac * 0.25, 0.0, fac / 16.0],
                    ADDITIVE_SCALE,
                )
            }
        }
    }
}

const HARMONIC_DOCS: ModuleDocs = ModuleDocs {
    summary: "Harmonic exciter",
    description: "Adds chosen harmonics to brighten or thicken a tone; on a \
                  sine it turns a pure tone organ-like or reedy. Modelled on \
                  the Surge XT harmonic shapes.",
    categories: &[ModuleCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "Sine to organ",
        code: "$sine('c3').$.unstable.shape.harmonic('add12345', 2).out()",
    }],
};

shape_module! {
    docs = HARMONIC_DOCS,
    name = "$unstable.shape.harmonic", ident = Harmonic, mode = HarmonicMode, shaper = HarmonicShaper
}
