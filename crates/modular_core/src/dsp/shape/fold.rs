//! `$unstable.shape.fold` — Surge XT's Wavefolder group.
//!
//! Ported from sst-waveshapers (GPL-3.0-or-later).

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::shape::shape_core::Shaper;
use crate::dsp::shape::shapers;
use crate::dsp::utils::adaa::Adaa;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Wavefolding algorithm.
#[derive(Clone, Copy, Debug, Default, Deserr, JsonSchema, PartialEq, Eq, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum FoldMode {
    /// a single, smooth fold
    #[default]
    Soft,
    /// a single sharp fold
    Single,
    /// a double fold, more harmonics
    Dual,
    /// west-coast (Buchla-style) fold with complex, metallic harmonics
    Westcoast,
    /// triangle fold, hard and buzzy
    Linear,
    /// sine fold, wraps the signal smoothly through a sine
    Sine,
}

/// Holds the ADAA memory the piecewise folders need, and the mode that memory
/// belongs to — the memory resets on a mode change (see [`Adaa::reset`]).
#[derive(Clone, Copy, Default)]
pub struct FoldShaper {
    adaa: Adaa,
    adaa_mode: FoldMode,
}

impl Shaper for FoldShaper {
    type Mode = FoldMode;

    #[inline]
    fn shape(&mut self, x: f32, drive: f32, mode: FoldMode, _dc: f32) -> f32 {
        if mode != self.adaa_mode {
            self.adaa.reset();
            self.adaa_mode = mode;
        }
        match mode {
            FoldMode::Soft => shapers::soft_one_fold(x, drive),
            FoldMode::Single => {
                let d = x * drive;
                let (f, ad) = shapers::SINGLE_FOLD.evaluate(d);
                self.adaa.process(d, f, ad)
            }
            FoldMode::Dual => {
                let d = x * drive;
                let (f, ad) = shapers::DUAL_FOLD.evaluate(d);
                self.adaa.process(d, f, ad)
            }
            FoldMode::Westcoast => {
                let d = x * drive;
                let (f, ad) = shapers::WESTCOAST_FOLD.evaluate(d);
                self.adaa.process(d, f, ad)
            }
            FoldMode::Linear => shapers::linear_fold(x, drive),
            FoldMode::Sine => shapers::sine_shaper(x, drive, true),
        }
    }
}

const FOLD_DOCS: ModuleDocs = ModuleDocs {
    summary: "Wavefolder",
    description: "Folds peaks back on themselves, adding harmonics that grow \
                  with level: the west-coast sound when an envelope sweeps the \
                  drive. Modelled on the Surge XT wavefolder shapes.",
    categories: &[ModuleCategory::Distortion],
    sections: &[],
    examples: &[DocExample {
        title: "West-coast pluck",
        code: r#"
            const seq = $p('c3 eb3 g3 bb2').c
            const env = $perc(seq, { decay: 0.4 })
            $sine(seq)
              .$.unstable.shape.fold('westcoast', env.range(-2, 4))
              .amp(env)
              .out()
        "#,
    }],
};

shape_module! {
    docs = FOLD_DOCS,
    name = "$unstable.shape.fold", ident = Fold, mode = FoldMode, shaper = FoldShaper
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::shape::shape_core::{ShapeChannel, ShapeModuleState, run};
    use crate::poly::{PolyOutput, PolySignal};
    use crate::types::Signal;

    /// The ADAA memory belongs to one fold shape; switching modes mid-stream must
    /// re-seed it. Differencing the new antiderivative against the old memory
    /// divides their offset by a near-zero step — a multi-hundred-volt spike.
    #[test]
    fn mode_switch_does_not_spike() {
        let sr = 48_000.0;
        let mut state = ShapeModuleState::default();
        state.init(sr);
        let mut ch: Vec<ShapeChannel<FoldShaper>> = vec![ShapeChannel::default()];
        let mut out = PolyOutput::mono(0.0);
        let drive = Some(PolySignal::mono(Signal::Volts(3.0)));
        let sig = |i: usize| (2.0 * std::f32::consts::PI * 220.0 * (i as f32 / sr)).sin() * 5.0;

        for i in 0..4000 {
            let input = PolySignal::mono(Signal::Volts(sig(i)));
            run::<FoldShaper>(
                1,
                &input,
                &drive,
                FoldMode::Dual,
                &state,
                &mut ch,
                &mut out,
                sr,
            );
        }
        for i in 0..256 {
            let input = PolySignal::mono(Signal::Volts(sig(4000 + i)));
            run::<FoldShaper>(
                1,
                &input,
                &drive,
                FoldMode::Westcoast,
                &state,
                &mut ch,
                &mut out,
                sr,
            );
            let y = out.get(0);
            assert!(
                y.abs() < 8.0,
                "fold mode switch spiked: {y} at sample {i} after the switch"
            );
        }
    }
}
