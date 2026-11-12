//! `$unstable.filter.cutoffWarp{Lp,Hp,Bp,Notch,Ap}` — Surge XT's Cutoff Warp
//! (`fut_cutoffwarp_*`): an RBJ biquad with a saturator in the *feedback* path
//! (Jatin Chowdhury's nonlinear-feedback design), cascadable up to four stages.
//!
//! Ported from https://github.com/surge-synthesizer/sst-filters (GPL-3.0).

use deserr::Deserr;
use schemars::JsonSchema;

use super::coeffs::note_to_hz;
use super::fastmath::{fastcos, fastsin, fasttanh_clamped, softclip};
use super::filter_core::{Filter, N_COEFFS, N_REGISTERS};
use crate::dsp::shape::shapers::ojd;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Biquad passband shared by the warp families.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WarpPassband {
    Lp,
    Hp,
    Notch,
    Bp,
    Ap,
}

/// Cutoff clamp shared by the warp families.
#[inline]
pub(super) fn warp_clamped_frequency(freq_semi: f32, rate: f32) -> f32 {
    note_to_hz(freq_semi).clamp(5.0, rate * 0.3)
}

/// Feedback saturator. Rendered as `'tanh'` / `'softClip'` / `'ojd'` in the DSL.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum WarpDrive {
    /// smooth, warm saturation
    #[default]
    Tanh,
    /// firmer, brighter saturation
    SoftClip,
    /// pedal-style overdrive that rounds off one side early and clips the other hard
    Ojd,
}

/// Cutoff-warp coefficient slots (Surge `nlf_coeffs`).
const NLF_A1: usize = 0;
const NLF_A2: usize = 1;
const NLF_B0: usize = 2;
const NLF_B1: usize = 3;
const NLF_B2: usize = 4;
const NLF_MAKEUP: usize = 5;
const N_NLF_COEFF: usize = 6;

/// Passband + Surge subtype (`(stages−1) | saturator<<2`, 0–11).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutoffWarpMode {
    pub passband: WarpPassband,
    pub subtype: i32,
}

/// Surge `CutoffWarp::doNLFilter`: one biquad step with the saturator applied to the
/// feedback signal only.
#[inline]
fn do_nl_filter(input: f32, c: &[f32; N_COEFFS], sat: i32, z1: &mut f32, z2: &mut f32) -> f32 {
    let [a1, a2, b0, b1, b2] = [c[NLF_A1], c[NLF_A2], c[NLF_B0], c[NLF_B1], c[NLF_B2]];
    let out = *z1 + b0 * input;
    let nf = match sat {
        1 => softclip(out),
        2 => ojd(out, 1.0),
        _ => fasttanh_clamped(out),
    };
    *z1 = *z2 + b1 * input - a1 * nf;
    *z2 = b2 * input - a2 * nf;
    out * c[NLF_MAKEUP]
}

/// Cutoff-warp kernel. Every subtype saturates per sample → 2× oversampled.
#[derive(Clone, Copy, Default)]
pub struct CutoffWarp;

impl Filter for CutoffWarp {
    type Mode = CutoffWarpMode;
    type Extra = ();

    /// Surge `CutoffWarp::makeCoefficients`: RBJ coefficients with `q = 18·reso³+0.1`
    /// and ear-tuned makeup gain (per-subtype table for LP/HP, extra resonance makeup
    /// for the OJD subtypes).
    fn coeffs(mode: CutoffWarpMode, freq_semi: f32, reso: f32, rate: f32) -> [f32; N_COEFFS] {
        /// Surge XT's RMS-measured, hand-tweaked LP/HP normalization per subtype.
        const LP_NORM_TABLE: [f32; 12] = [
            1.53273, 1.33407, 1.08197, 0.958219, 1.27374, 0.932342, 0.761765, 0.665462, 0.776856,
            0.597575, 0.496207, 0.471714,
        ];

        let reso = reso.clamp(0.0, 1.0);
        let q = (reso * reso * reso) * 18.0 + 0.1;

        let normalised_freq = 2.0 * warp_clamped_frequency(freq_semi, rate) / rate;
        let wc = std::f32::consts::PI * normalised_freq;

        let wsin = fastsin(wc);
        let wcos = fastcos(wc);
        let alpha = wsin / (2.0 * q);
        let a0r = 1.0 / (1.0 + alpha);

        let mut c = [0.0f32; N_COEFFS];
        c[NLF_A1] = -2.0 * wcos * a0r;
        c[NLF_A2] = (1.0 - alpha) * a0r;
        c[NLF_MAKEUP] = 1.0;

        let exp_min = if matches!(mode.passband, WarpPassband::Lp) {
            0.1
        } else {
            0.35
        };
        let res_makeup = if mode.subtype < 8 {
            1.0
        } else {
            1.0 / reso.max(exp_min).sqrt()
        };

        match mode.passband {
            WarpPassband::Lp => {
                c[NLF_B1] = (1.0 - wcos) * a0r;
                c[NLF_B0] = c[NLF_B1] * 0.5;
                c[NLF_B2] = c[NLF_B0];
                c[NLF_MAKEUP] = res_makeup * LP_NORM_TABLE[mode.subtype as usize]
                    / normalised_freq.max(0.001).powf(0.1);
            }
            WarpPassband::Hp => {
                c[NLF_B1] = -(1.0 + wcos) * a0r;
                c[NLF_B0] = c[NLF_B1] * -0.5;
                c[NLF_B2] = c[NLF_B0];
                c[NLF_MAKEUP] = res_makeup * LP_NORM_TABLE[mode.subtype as usize]
                    / (1.0 - normalised_freq).max(0.001).powf(0.1);
            }
            WarpPassband::Notch => {
                c[NLF_B0] = a0r;
                c[NLF_B1] = -2.0 * wcos * a0r;
                c[NLF_B2] = c[NLF_B0];
            }
            WarpPassband::Bp => {
                c[NLF_B0] = wsin * 0.5 * a0r;
                c[NLF_B1] = 0.0;
                c[NLF_B2] = -c[NLF_B0];
            }
            WarpPassband::Ap => {
                c[NLF_B0] = c[NLF_A2];
                c[NLF_B1] = c[NLF_A1];
                c[NLF_B2] = 1.0;
            }
        }
        c
    }

    fn process(
        mode: CutoffWarpMode,
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
            out = do_nl_filter(out, c, sat, &mut z1, &mut z2);
            r[stage * 2] = z1;
            r[stage * 2 + 1] = z2;
        }

        for i in 0..N_NLF_COEFF {
            c[i] += dc[i];
        }
        out
    }

    fn oversample(_mode: CutoffWarpMode) -> bool {
        true
    }
}

/// Stamp one cutoff-warp passband module.
macro_rules! cutoff_warp_module {
    ($docs:path, $name:literal, $Struct:ident, $passband:expr, $output_doc:literal, $cutoff_doc:literal) => {
        filter_module! {
            docs = $docs,
            name = $name, ident = $Struct, kernel = CutoffWarp,
            output_doc = $output_doc,
            cutoff_doc = $cutoff_doc,
            params = {
                #[input("saturation in the feedback path", default = "tanh")]
                drive: WarpDrive,
                #[input("number of cascaded filter stages", default = 1, range = (1, 4))]
                stages: Option<usize>,
            },
            mode = |p| CutoffWarpMode {
                passband: $passband,
                subtype: (p.stages.unwrap_or(1).clamp(1, 4) as i32 - 1) | ((p.drive as i32) << 2),
            },
        }
    };
}

const CUTOFF_WARP_LP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Cutoff Warp lowpass",
    description: "A resonant lowpass with saturation in its feedback, so drive bends the sweep \
                  into a hot, distorted, slightly unstable growl; `resWarpLp` squashes the \
                  resonant peak instead. More `stages` cut steeper and saturate harder.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Growling saturated sweep",
        code: r#"
            $unstable.filter
              .cutoffWarpLp(
                $saw('c2'),
                $sine('0.2hz').range('c3', 'c6'),
                3,
                { drive: 'ojd', stages: 2 },
              )
              .out()
        "#,
    }],
};

const CUTOFF_WARP_HP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Cutoff Warp highpass",
    description: "A resonant highpass with saturation in its feedback, for a thin, crackling, \
                  overdriven top end; `resWarpHp` thickens the peak instead.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Crackling highpass sweep",
        code: r#"
            $unstable.filter
              .cutoffWarpHp(
                $saw('c2'),
                $sine('0.2hz').range('c3', 'c6'),
                3,
                { stages: 2 },
              )
              .out()
        "#,
    }],
};

const CUTOFF_WARP_BP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Cutoff Warp bandpass",
    description: "A resonant bandpass with saturation in its feedback: a narrow, distorted \
                  band that honks when driven, where `resWarpBp` buzzes.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Honking distorted wah",
        code: r#"
            $unstable.filter
              .cutoffWarpBp(
                $saw('c2'),
                $sine('1hz').range('c4', 'c6'),
                3,
              )
              .out()
        "#,
    }],
};

const CUTOFF_WARP_NOTCH_DOCS: ModuleDocs = ModuleDocs {
    summary: "Cutoff Warp notch",
    description: "A notch with saturation in its feedback, so the hole it cuts smears and \
                  grits up with level.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Gritty notch sweep",
        code: r#"
            $unstable.filter
              .cutoffWarpNotch(
                $saw('c2'),
                $sine('0.2hz').range('c3', 'c6'),
                3,
              )
              .out()
        "#,
    }],
};

const CUTOFF_WARP_AP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Cutoff Warp allpass",
    description: "An allpass with saturation in its feedback: phase-smearing that adds its own \
                  distortion; stack `stages` for a deeper smear.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Driven phase smear",
        code: r#"
            const saw = $saw('c2')
            const sweep = $sine('0.2hz').range('c3', 'c6')
            const wet = $unstable.filter.cutoffWarpAp(saw, sweep, 3, {
              drive: 'softClip',
              stages: 4,
            })
            $mix([saw, wet]).out()
        "#,
    }],
};

cutoff_warp_module!(
    CUTOFF_WARP_LP_DOCS,
    "$unstable.filter.cutoffWarpLp",
    CutoffWarpLpFilter,
    WarpPassband::Lp,
    "lowpass output",
    "cutoff frequency"
);
cutoff_warp_module!(
    CUTOFF_WARP_HP_DOCS,
    "$unstable.filter.cutoffWarpHp",
    CutoffWarpHpFilter,
    WarpPassband::Hp,
    "highpass output",
    "cutoff frequency"
);
cutoff_warp_module!(
    CUTOFF_WARP_BP_DOCS,
    "$unstable.filter.cutoffWarpBp",
    CutoffWarpBpFilter,
    WarpPassband::Bp,
    "bandpass output",
    "centre frequency of the band"
);
cutoff_warp_module!(
    CUTOFF_WARP_NOTCH_DOCS,
    "$unstable.filter.cutoffWarpNotch",
    CutoffWarpNotchFilter,
    WarpPassband::Notch,
    "notch output",
    "centre frequency of the notch"
);
cutoff_warp_module!(
    CUTOFF_WARP_AP_DOCS,
    "$unstable.filter.cutoffWarpAp",
    CutoffWarpApFilter,
    WarpPassband::Ap,
    "allpass output",
    "frequency the phase shift centres on"
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::filters::surge::filter_core::test_util::{sine_rms, sweep_stays_bounded};

    const SR: f32 = 96_000.0; // kernel run rate is 2× the 48 kHz engine rate

    fn all_modes(passband: WarpPassband) -> Vec<CutoffWarpMode> {
        (0..12)
            .map(|subtype| CutoffWarpMode { passband, subtype })
            .collect()
    }

    fn rms(mode: CutoffWarpMode, freq: f32) -> f32 {
        // 1 kHz cutoff expressed in semitones above A440.
        let cutoff_semi = 12.0 * (1000.0f32 / 440.0).log2();
        sine_rms::<CutoffWarp>(mode, cutoff_semi, 0.2, freq, SR)
    }

    #[test]
    fn passband_shapes_all_subtypes() {
        for subtype in 0..12 {
            let lp = |f| {
                rms(
                    CutoffWarpMode {
                        passband: WarpPassband::Lp,
                        subtype,
                    },
                    f,
                )
            };
            assert!(lp(8000.0) < lp(200.0) * 0.5, "lp shape (subtype {subtype})");
            let hp = |f| {
                rms(
                    CutoffWarpMode {
                        passband: WarpPassband::Hp,
                        subtype,
                    },
                    f,
                )
            };
            assert!(hp(125.0) < hp(8000.0) * 0.5, "hp shape (subtype {subtype})");
            let bp = |f| {
                rms(
                    CutoffWarpMode {
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
                    CutoffWarpMode {
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
            sweep_stays_bounded::<CutoffWarp>(&all_modes(passband));
        }
    }
}
