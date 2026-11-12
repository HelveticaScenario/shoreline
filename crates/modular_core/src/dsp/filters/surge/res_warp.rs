//! `$unstable.filter.resWarp{Lp,Hp,Bp,Notch,Ap}` — Surge XT's Resonance Warp
//! (`fut_resonancewarp_*`): an RBJ biquad with saturators on the *state* variables
//! (Jatin Chowdhury's nonlinear-biquad design), cascadable up to four stages.
//!
//! Ported from https://github.com/surge-synthesizer/sst-filters (GPL-3.0).

use deserr::Deserr;
use schemars::JsonSchema;

use super::cutoff_warp::{WarpPassband, warp_clamped_frequency};
use super::fastmath::{fastcos, fastsin, fasttanh_clamped, softclip};
use super::filter_core::{Filter, N_COEFFS, N_REGISTERS};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Res-warp coefficient slots (Surge `nls_coeffs`).
const NLS_A1: usize = 0;
const NLS_A2: usize = 1;
const NLS_B0: usize = 2;
const NLS_B1: usize = 3;
const NLS_B2: usize = 4;
const N_NLS_COEFF: usize = 5;

/// State saturator. Rendered as `'tanh'` / `'softClip'` in the DSL.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum ResWarpDrive {
    /// smooth, warm saturation
    #[default]
    Tanh,
    /// firmer, brighter saturation
    SoftClip,
}

/// Passband + Surge subtype (`(stages−1) | saturator<<2`, 0–7).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResWarpMode {
    pub passband: WarpPassband,
    pub subtype: i32,
}

/// Surge `ResonanceWarp::doNLFilter`: one biquad step with the saturator applied to
/// both state variables after the update.
#[inline]
fn do_nl_filter(
    input: f32,
    a1: f32,
    a2: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    sat: i32,
    z1: &mut f32,
    z2: &mut f32,
) -> f32 {
    let out = *z1 + b0 * input;
    *z1 = *z2 + b1 * input - a1 * out;
    *z2 = b2 * input - a2 * out;
    if sat == 0 {
        *z1 = fasttanh_clamped(*z1);
        *z2 = fasttanh_clamped(*z2);
    } else {
        *z1 = softclip(*z1);
        *z2 = softclip(*z2);
    }
    out
}

/// Res-warp kernel. Every subtype saturates per sample → 2× oversampled.
#[derive(Clone, Copy, Default)]
pub struct ResWarp;

impl Filter for ResWarp {
    type Mode = ResWarpMode;
    type Extra = ();

    /// Surge `ResonanceWarp::makeCoefficients`: the same RBJ build as Cutoff Warp
    /// (`q = 18·reso³ + 0.1`), with no makeup gain.
    fn coeffs(mode: ResWarpMode, freq_semi: f32, reso: f32, rate: f32) -> [f32; N_COEFFS] {
        let reso = reso.clamp(0.0, 1.0);
        let q = (reso * reso * reso) * 18.0 + 0.1;

        let wc = 2.0 * std::f32::consts::PI * warp_clamped_frequency(freq_semi, rate) / rate;
        let wsin = fastsin(wc);
        let wcos = fastcos(wc);
        let alpha = wsin / (2.0 * q);
        let a0r = 1.0 / (1.0 + alpha);

        let mut c = [0.0f32; N_COEFFS];
        c[NLS_A1] = -2.0 * wcos * a0r;
        c[NLS_A2] = (1.0 - alpha) * a0r;

        match mode.passband {
            WarpPassband::Lp => {
                c[NLS_B1] = (1.0 - wcos) * a0r;
                c[NLS_B0] = c[NLS_B1] * 0.5;
                c[NLS_B2] = c[NLS_B0];
            }
            WarpPassband::Hp => {
                c[NLS_B1] = -(1.0 + wcos) * a0r;
                c[NLS_B0] = c[NLS_B1] * -0.5;
                c[NLS_B2] = c[NLS_B0];
            }
            WarpPassband::Notch => {
                c[NLS_B0] = a0r;
                c[NLS_B1] = -2.0 * wcos * a0r;
                c[NLS_B2] = c[NLS_B0];
            }
            WarpPassband::Bp => {
                c[NLS_B0] = wsin * 0.5 * a0r;
                c[NLS_B1] = 0.0;
                c[NLS_B2] = -c[NLS_B0];
            }
            WarpPassband::Ap => {
                c[NLS_B0] = c[NLS_A2];
                c[NLS_B1] = c[NLS_A1];
                c[NLS_B2] = 1.0;
            }
        }
        c
    }

    fn process(
        mode: ResWarpMode,
        x: f32,
        c: &mut [f32; N_COEFFS],
        dc: &[f32; N_COEFFS],
        r: &mut [f32; N_REGISTERS],
        _extra: &mut (),
    ) -> f32 {
        let stages = (mode.subtype & 3) as usize;
        let sat = (mode.subtype >> 2) & 3;

        let mut out = x;
        for stage in 0..=stages {
            let mut z1 = r[stage * 2];
            let mut z2 = r[stage * 2 + 1];
            out = do_nl_filter(
                out, c[NLS_A1], c[NLS_A2], c[NLS_B0], c[NLS_B1], c[NLS_B2], sat, &mut z1, &mut z2,
            );
            r[stage * 2] = z1;
            r[stage * 2 + 1] = z2;
        }

        for i in 0..N_NLS_COEFF {
            c[i] += dc[i];
        }
        out
    }

    fn oversample(_mode: ResWarpMode) -> bool {
        true
    }
}

/// Stamp one res-warp passband module.
macro_rules! res_warp_module {
    ($docs:path, $name:literal, $Struct:ident, $passband:expr, $output_doc:literal, $cutoff_doc:literal) => {
        filter_module! {
            docs = $docs,
            name = $name, ident = $Struct, kernel = ResWarp,
            output_doc = $output_doc,
            cutoff_doc = $cutoff_doc,
            params = {
                #[input("saturation inside the filter", default = "tanh")]
                drive: ResWarpDrive,
                #[input("number of cascaded filter stages", default = 1, range = (1, 4))]
                stages: Option<usize>,
            },
            mode = |p| ResWarpMode {
                passband: $passband,
                subtype: (p.stages.unwrap_or(1).clamp(1, 4) as i32 - 1) | ((p.drive as i32) << 2),
            },
        }
    };
}

const RES_WARP_LP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonance Warp lowpass",
    description: "A resonant lowpass with saturation inside the filter, so loud signals squash \
                  and warp the resonant peak into a thick, rubbery tone, where `cutoffWarpLp` \
                  growls as level pushes its cutoff around. More `stages` cut steeper.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Rubbery resonant bass",
        code: r#"
            const seq = $p('f1 [~ f2] ab1 [c2 eb2]')
            $unstable.filter
              .resWarpLp(
                $saw(seq),
                $perc(seq, { decay: 0.2 }).range('c3', 'c6'),
                4,
                { stages: 2 },
              )
              .out()
        "#,
    }],
};

const RES_WARP_HP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonance Warp highpass",
    description: "A resonant highpass with saturation inside the filter: the peak thickens and \
                  warps instead of whistling as it does in `cutoffWarpHp`.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Thick highpass sweep",
        code: r#"
            $unstable.filter
              .resWarpHp(
                $saw('c2'),
                $sine('0.2hz').range('c3', 'c6'),
                4,
              )
              .out()
        "#,
    }],
};

const RES_WARP_BP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonance Warp bandpass",
    description: "A resonant bandpass with saturation inside the filter: a squashed, buzzing \
                  band.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Buzzing band sweep",
        code: r#"
            $unstable.filter
              .resWarpBp(
                $saw('c2'),
                $sine('0.5hz').range('c4', 'c6'),
                4,
              )
              .out()
        "#,
    }],
};

const RES_WARP_NOTCH_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonance Warp notch",
    description: "A notch with saturation inside the filter, its edges warping with level.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Warped notch sweep",
        code: r#"
            $unstable.filter
              .resWarpNotch(
                $saw('c2'),
                $sine('0.2hz').range('c3', 'c6'),
                3,
              )
              .out()
        "#,
    }],
};

const RES_WARP_AP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonance Warp allpass",
    description: "An allpass with saturation inside the filter, for a saturated phase smear; \
                  stack `stages` to deepen it.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Saturated phaser",
        code: r#"
            const saw = $saw('c2')
            $mix([
              saw,
              $unstable.filter.resWarpAp(
                saw,
                $sine('0.2hz').range('c3', 'c6'),
                3,
                { stages: 3 },
              ),
            ]).out()
        "#,
    }],
};

res_warp_module!(
    RES_WARP_LP_DOCS,
    "$unstable.filter.resWarpLp",
    ResWarpLpFilter,
    WarpPassband::Lp,
    "lowpass output",
    "cutoff frequency"
);
res_warp_module!(
    RES_WARP_HP_DOCS,
    "$unstable.filter.resWarpHp",
    ResWarpHpFilter,
    WarpPassband::Hp,
    "highpass output",
    "cutoff frequency"
);
res_warp_module!(
    RES_WARP_BP_DOCS,
    "$unstable.filter.resWarpBp",
    ResWarpBpFilter,
    WarpPassband::Bp,
    "bandpass output",
    "centre frequency of the band"
);
res_warp_module!(
    RES_WARP_NOTCH_DOCS,
    "$unstable.filter.resWarpNotch",
    ResWarpNotchFilter,
    WarpPassband::Notch,
    "notch output",
    "centre frequency of the notch"
);
res_warp_module!(
    RES_WARP_AP_DOCS,
    "$unstable.filter.resWarpAp",
    ResWarpApFilter,
    WarpPassband::Ap,
    "allpass output",
    "frequency the phase shift centres on"
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::filters::surge::filter_core::test_util::{sine_rms_amp, sweep_stays_bounded};

    const SR: f32 = 96_000.0; // kernel run rate is 2× the 48 kHz engine rate

    fn all_modes(passband: WarpPassband) -> Vec<ResWarpMode> {
        (0..8)
            .map(|subtype| ResWarpMode { passband, subtype })
            .collect()
    }

    /// Small-signal probe: the state saturators make the response level-dependent
    /// (full-scale input compresses the states and flattens the shape), so the
    /// passband shape is verified in the linear regime.
    fn rms(mode: ResWarpMode, freq: f32) -> f32 {
        // 1 kHz cutoff expressed in semitones above A440.
        let cutoff_semi = 12.0 * (1000.0f32 / 440.0).log2();
        sine_rms_amp::<ResWarp>(mode, cutoff_semi, 0.2, freq, SR, 0.05)
    }

    #[test]
    fn passband_shapes_all_subtypes() {
        for subtype in 0..8 {
            let lp = |f| {
                rms(
                    ResWarpMode {
                        passband: WarpPassband::Lp,
                        subtype,
                    },
                    f,
                )
            };
            assert!(lp(8000.0) < lp(200.0) * 0.5, "lp shape (subtype {subtype})");
            let hp = |f| {
                rms(
                    ResWarpMode {
                        passband: WarpPassband::Hp,
                        subtype,
                    },
                    f,
                )
            };
            assert!(hp(125.0) < hp(8000.0) * 0.5, "hp shape (subtype {subtype})");
            let bp = |f| {
                rms(
                    ResWarpMode {
                        passband: WarpPassband::Bp,
                        subtype,
                    },
                    f,
                )
            };
            let bp_c = bp(1000.0);
            assert!(
                bp(60.0) < bp_c * 0.5 && bp(12_000.0) < bp_c * 0.5,
                "bp shape (subtype {subtype})"
            );
            let n = |f| {
                rms(
                    ResWarpMode {
                        passband: WarpPassband::Notch,
                        subtype,
                    },
                    f,
                )
            };
            assert!(
                n(1000.0) < n(100.0) * 0.5 && n(1000.0) < n(10_000.0) * 0.5,
                "notch shape (subtype {subtype})"
            );
        }
    }

    #[test]
    fn all_passbands_survive_resonant_cutoff_sweep() {
        for passband in [
            WarpPassband::Lp,
            WarpPassband::Hp,
            WarpPassband::Bp,
            WarpPassband::Notch,
            WarpPassband::Ap,
        ] {
            sweep_stays_bounded::<ResWarp>(&all_modes(passband));
        }
    }
}
