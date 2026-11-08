//! `$unstable.shape.rectify` — Surge XT's Rectifier waveshaper group.
//!
//! Ported from sst-waveshapers (GPL-3.0-or-later).

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::shape::shape_core::Shaper;
use crate::dsp::shape::shapers;
use crate::dsp::utils::adaa::Adaa;
use crate::dsp::utils::dc_blocker::DcBlocker;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Rectification algorithm.
#[derive(Clone, Copy, Debug, Default, Deserr, JsonSchema, PartialEq, Eq, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum RectifyMode {
    /// full-wave, flips the negative half up an octave higher
    #[default]
    Full,
    /// positive half-wave, keeps only the upper half
    Positive,
    /// negative half-wave, keeps only the lower half
    Negative,
    /// softer rectification with rounded edges
    Soft,
}

/// Holds the ADAA memory (and, for soft, a DC blocker), plus the mode the ADAA
/// memory belongs to — the memory resets on a mode change (see [`Adaa::reset`]).
#[derive(Clone, Copy, Default)]
pub struct RectifyShaper {
    adaa: Adaa,
    adaa_mode: RectifyMode,
    dc: DcBlocker,
}

impl Shaper for RectifyShaper {
    type Mode = RectifyMode;

    #[inline]
    fn shape(&mut self, x: f32, drive: f32, mode: RectifyMode, dc: f32) -> f32 {
        if mode != self.adaa_mode {
            self.adaa.reset();
            self.adaa_mode = mode;
        }
        match mode {
            RectifyMode::Full => {
                let d = shapers::clip(x, drive);
                let (f, ad) = shapers::fwrect_kernel(d);
                self.adaa.process(d, f, ad)
            }
            RectifyMode::Positive => {
                let d = shapers::clip(x, drive);
                let (f, ad) = shapers::posrect_kernel(d);
                self.adaa.process(d, f, ad)
            }
            RectifyMode::Negative => {
                let d = shapers::clip(x, drive);
                let (f, ad) = shapers::negrect_kernel(d);
                self.adaa.process(d, f, ad)
            }
            RectifyMode::Soft => {
                let (f, ad) = shapers::softrect_kernel(x);
                let r = self.adaa.process(x, f, ad);
                let r = self.dc.process(r, dc);
                shapers::tanh(r, drive)
            }
        }
    }
}

const RECTIFY_DOCS: ModuleDocs = ModuleDocs {
    summary: "Rectifier",
    description: "Flips or cuts off half the waveform for a harder, buzzier \
                  tone; full-wave rectifying a sine lifts it an octave. \
                  Modelled on the Surge XT rectifier shapes.",
    categories: &[ModuleCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "Octave-up buzz",
        code: "$sine('c3').$.unstable.shape.rectify('full').out()",
    }],
};

shape_module! {
    docs = RECTIFY_DOCS,
    name = "$unstable.shape.rectify", ident = Rectify, mode = RectifyMode, shaper = RectifyShaper
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::shape::shape_core::{ShapeChannel, ShapeModuleState, run};
    use crate::poly::{PolyOutput, PolySignal};
    use crate::types::Signal;

    /// The ADAA memory belongs to one rectifier kernel; switching modes mid-stream
    /// must re-seed it (same spike mechanism as the fold shapes).
    #[test]
    fn mode_switch_does_not_spike() {
        let sr = 48_000.0;
        let mut state = ShapeModuleState::default();
        state.init(sr);
        let mut ch: Vec<ShapeChannel<RectifyShaper>> = vec![ShapeChannel::default()];
        let mut out = PolyOutput::mono(0.0);
        let drive = Some(PolySignal::mono(Signal::Volts(3.0)));
        let sig = |i: usize| (2.0 * std::f32::consts::PI * 220.0 * (i as f32 / sr)).sin() * 5.0;

        for i in 0..4000 {
            let input = PolySignal::mono(Signal::Volts(sig(i)));
            run::<RectifyShaper>(
                1,
                &input,
                &drive,
                RectifyMode::Full,
                &state,
                &mut ch,
                &mut out,
                sr,
            );
        }
        for i in 0..256 {
            let input = PolySignal::mono(Signal::Volts(sig(4000 + i)));
            run::<RectifyShaper>(
                1,
                &input,
                &drive,
                RectifyMode::Soft,
                &state,
                &mut ch,
                &mut out,
                sr,
            );
            let y = out.get(0);
            assert!(
                y.abs() < 8.0,
                "rectify mode switch spiked: {y} at sample {i} after the switch"
            );
        }
    }
}
