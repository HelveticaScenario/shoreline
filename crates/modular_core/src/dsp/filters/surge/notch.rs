//! `$unstable.filter.notch` — Surge XT's notch biquads (Notch 12/24 dB).
//!
//! Ported from https://github.com/surge-synthesizer/sst-filters (GPL-3.0).

use deserr::Deserr;
use schemars::JsonSchema;

use super::biquad::{FilterSlope, iir12_b, iir24_b};
use super::coeffs::{bound_freq, note_to_omega, to_normalized_lattice};
use super::filter_core::{Filter, N_COEFFS, N_REGISTERS};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Notch width character.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum NotchStyle {
    /// wide notch with a strong resonance response
    #[default]
    Standard,
    /// narrower, gentler notch
    Mild,
}

/// Slope + width pair selecting one concrete notch configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotchMode {
    pub four_pole: bool,
    pub mild: bool,
}

/// Notch biquad kernel. Both subtypes run the normalized-lattice form; the 24 dB
/// slope cascades the same section twice.
#[derive(Clone, Copy, Default)]
pub struct NotchBiquad;

impl Filter for NotchBiquad {
    type Mode = NotchMode;
    type Extra = ();

    fn coeffs(mode: NotchMode, freq_semi: f32, reso: f32, rate: f32) -> [f32; N_COEFFS] {
        coeff_notch(freq_semi, reso, mode.mild, rate)
    }

    fn process(
        mode: NotchMode,
        x: f32,
        c: &mut [f32; N_COEFFS],
        dc: &[f32; N_COEFFS],
        r: &mut [f32; N_REGISTERS],
        _extra: &mut (),
    ) -> f32 {
        if mode.four_pole {
            iir24_b(x, c, dc, r)
        } else {
            iir12_b(x, c, dc, r)
        }
    }
}

/// Surge `Coeff_Notch`: unit-gain notch numerator (`b0 = 1, b1 = -2cos, b2 = 1`)
/// → lattice form with a fixed 0.005 clip gain. Resonance widens the notch
/// (Standard) or narrows it (Mild).
fn coeff_notch(freq_semi: f32, reso: f32, mild: bool, rate: f32) -> [f32; N_COEFFS] {
    let freq = bound_freq(freq_semi);
    let (sinu, cosi) = note_to_omega(freq, rate);
    let (sinu, cosi) = (sinu as f64, cosi as f64);

    let reso = reso as f64;
    let warp = (1.0 - (1.0 - reso) * (1.0 - reso)).clamp(0.0, 1.0);
    let q2inv = if mild {
        1.00 - 0.99 * warp
    } else {
        2.5 - 2.49 * warp
    };

    let alpha = sinu * q2inv;

    let a0 = 1.0 + alpha;
    let a0inv = 1.0 / a0;
    let a1 = -2.0 * cosi;
    let a2 = 1.0 - alpha;
    let b0 = 1.0;
    let b1 = -2.0 * cosi;
    let b2 = 1.0;

    to_normalized_lattice(a0inv, a1, a2, b0, b1, b2, 0.005)
}

const NOTCH_DOCS: ModuleDocs = ModuleDocs {
    summary: "Surge notch filter",
    description: "Cuts a hole at the cutoff and leaves the rest; swept slowly it gives a \
                  hollow, phaser-like motion.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Hollow notch sweep",
        code: r#"
            $unstable.filter
              .notch($saw('c2'), $sine('0.2hz').range('c3', 'c6'), 1)
              .out()
        "#,
    }],
};

filter_module! {
    docs = NOTCH_DOCS,
    name = "$unstable.filter.notch", ident = NotchFilter, kernel = NotchBiquad,
    output_doc = "notch output",
    cutoff_doc = "centre frequency of the notch",
    params = {
        #[input("how steeply it cuts", default = "db24")]
        slope: FilterSlope,
        #[input("how wide and strong the notch is", default = "standard")]
        style: NotchStyle,
    },
    mode = |p| NotchMode {
        four_pole: p.slope.four_pole(),
        mild: matches!(p.style, NotchStyle::Mild),
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::filters::surge::filter_core::test_util::{sine_rms, sweep_stays_bounded};

    fn all_modes() -> Vec<NotchMode> {
        let mut v = Vec::new();
        for four_pole in [false, true] {
            for mild in [false, true] {
                v.push(NotchMode { four_pole, mild });
            }
        }
        v
    }

    #[test]
    fn notch_nulls_center_passes_skirts_all_subtypes() {
        let sr = 48_000.0;
        // 1 kHz center expressed in semitones above A440.
        let cutoff_semi = 12.0 * (1000.0f32 / 440.0).log2();
        for mode in all_modes() {
            let center = sine_rms::<NotchBiquad>(mode, cutoff_semi, 0.5, 1000.0, sr);
            let low = sine_rms::<NotchBiquad>(mode, cutoff_semi, 0.5, 100.0, sr);
            let high = sine_rms::<NotchBiquad>(mode, cutoff_semi, 0.5, 10_000.0, sr);
            assert!(
                center < low * 0.3 && center < high * 0.3,
                "expected notch shape (mode {mode:?}): low={low} center={center} high={high}"
            );
        }
    }

    #[test]
    fn all_modes_survive_resonant_cutoff_sweep() {
        sweep_stays_bounded::<NotchBiquad>(&all_modes());
    }
}
