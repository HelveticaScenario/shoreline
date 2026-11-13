//! `$unstable.filter.lp` — Surge XT's multimode lowpass biquads (LP 12/24 dB).
//!
//! Ported from https://github.com/surge-synthesizer/sst-filters (GPL-3.0).

use super::biquad::{
    BiquadDrive, BiquadMode, FilterSlope, iir12_b, iir12_cfc, iir24_b, iir24_cfc, svf_lp12,
    svf_lp24,
};
use super::coeffs::{
    ST_CLEAN, ST_STANDARD, bound_freq, clipscale, coeff_svf, map_2pole_resonance,
    map_4pole_resonance, note_to_omega, resoscale, to_coupled_form, to_normalized_lattice,
};
use super::filter_core::{Filter, N_COEFFS, N_REGISTERS};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Lowpass biquad kernel.
#[derive(Clone, Copy, Default)]
pub struct LpBiquad;

impl Filter for LpBiquad {
    type Mode = BiquadMode;
    type Extra = ();

    fn coeffs(mode: BiquadMode, freq_semi: f32, reso: f32, rate: f32) -> [f32; N_COEFFS] {
        if mode.subtype == ST_STANDARD {
            coeff_svf(freq_semi, reso, mode.four_pole, rate)
        } else if mode.four_pole {
            coeff_lp24(freq_semi, reso, mode.subtype, rate)
        } else {
            coeff_lp12(freq_semi, reso, mode.subtype, rate)
        }
    }

    fn process(
        mode: BiquadMode,
        x: f32,
        c: &mut [f32; N_COEFFS],
        dc: &[f32; N_COEFFS],
        r: &mut [f32; N_REGISTERS],
        _extra: &mut (),
    ) -> f32 {
        match (mode.subtype, mode.four_pole) {
            (ST_STANDARD, false) => svf_lp12(x, c, dc, r),
            (ST_STANDARD, true) => svf_lp24(x, c, dc, r),
            (ST_CLEAN, false) => iir12_b(x, c, dc, r),
            (ST_CLEAN, true) => iir24_b(x, c, dc, r),
            (_, false) => iir12_cfc(x, c, dc, r),
            (_, true) => iir24_cfc(x, c, dc, r),
        }
    }
}

/// Surge `Coeff_LP12` (Driven/Clean path): biquad `a`/`b` → coupled/lattice form.
/// Unlike HP/BP, the alpha stability clamp exempts the Clean subtype.
fn coeff_lp12(freq_semi: f32, reso: f32, subtype: i32, rate: f32) -> [f32; N_COEFFS] {
    let gain = resoscale(reso, subtype) as f64;
    let freq = bound_freq(freq_semi);
    let (sinu, cosi) = note_to_omega(freq, rate);
    let (sinu, cosi) = (sinu as f64, cosi as f64);

    let mut alpha = sinu * map_2pole_resonance(reso as f64, freq as f64, subtype);
    if subtype != ST_CLEAN {
        alpha = alpha.min((1.0 - cosi * cosi).sqrt() - 0.0001);
    }

    let a0 = 1.0 + alpha;
    let a0inv = 1.0 / a0;
    let a1 = -2.0 * cosi;
    let a2 = 1.0 - alpha;
    let b0 = (1.0 - cosi) * 0.5;
    let b1 = 1.0 - cosi;
    let b2 = (1.0 - cosi) * 0.5;

    let g = clipscale(freq, subtype);
    if subtype == ST_CLEAN {
        to_normalized_lattice(a0inv, a1, a2, b0 * gain, b1 * gain, b2 * gain, g)
    } else {
        to_coupled_form(a0inv, a1, a2, b0 * gain, b1 * gain, b2 * gain, g)
    }
}

/// Surge `Coeff_LP24`: identical to `Coeff_LP12` but with the 4-pole resonance map.
fn coeff_lp24(freq_semi: f32, reso: f32, subtype: i32, rate: f32) -> [f32; N_COEFFS] {
    let gain = resoscale(reso, subtype) as f64;
    let freq = bound_freq(freq_semi);
    let (sinu, cosi) = note_to_omega(freq, rate);
    let (sinu, cosi) = (sinu as f64, cosi as f64);

    let mut alpha = sinu * map_4pole_resonance(reso as f64, freq as f64, subtype);
    if subtype != ST_CLEAN {
        alpha = alpha.min((1.0 - cosi * cosi).sqrt() - 0.0001);
    }

    let a0 = 1.0 + alpha;
    let a0inv = 1.0 / a0;
    let a1 = -2.0 * cosi;
    let a2 = 1.0 - alpha;
    let b0 = (1.0 - cosi) * 0.5;
    let b1 = 1.0 - cosi;
    let b2 = (1.0 - cosi) * 0.5;

    let g = clipscale(freq, subtype);
    if subtype == ST_CLEAN {
        to_normalized_lattice(a0inv, a1, a2, b0 * gain, b1 * gain, b2 * gain, g)
    } else {
        to_coupled_form(a0inv, a1, a2, b0 * gain, b1 * gain, b2 * gain, g)
    }
}

const LP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Surge lowpass filter",
    description: "Surge's resonant lowpass in 12 or 24 dB slopes: smooth at the default \
                  `'standard'` drive, precise at `'clean'`, gritty and singing at `'driven'`.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Driven acid line",
        code: r#"
            const seq = $p('c2 [c2 c3] eb2 [g1 bb1]')
            const env = $perc(seq, { decay: 0.2 })
            $unstable.filter
              .lp($saw(seq), env.range('c3', 'c6'), 4, {
                drive: 'driven',
              })
              .out()
        "#,
    }],
};

filter_module! {
    docs = LP_DOCS,
    name = "$unstable.filter.lp", ident = LpFilter, kernel = LpBiquad,
    output_doc = "lowpass output",
    cutoff_doc = "cutoff frequency",
    params = {
        #[input("how steeply it cuts", default = "db24")]
        slope: FilterSlope,
        #[input("how hard the resonance pushes and clips", default = "standard")]
        drive: BiquadDrive,
    },
    mode = |p| BiquadMode {
        four_pole: p.slope.four_pole(),
        subtype: p.drive.subtype(),
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::filters::surge::coeffs::ST_DRIVEN;
    use crate::dsp::filters::surge::filter_core::test_util::{sine_rms, sweep_stays_bounded};
    use crate::dsp::filters::surge::filter_core::{
        FilterInputs, FilterLane, FilterModuleState, on_patch_update, run,
    };

    fn all_modes() -> Vec<BiquadMode> {
        let mut v = Vec::new();
        for four_pole in [false, true] {
            for subtype in [ST_STANDARD, ST_DRIVEN, ST_CLEAN] {
                v.push(BiquadMode { four_pole, subtype });
            }
        }
        v
    }

    #[test]
    fn lowpass_attenuates_highs_all_subtypes() {
        let sr = 48_000.0;
        // 1 kHz cutoff expressed in semitones above A440.
        let cutoff_semi = 12.0 * (1000.0f32 / 440.0).log2();
        for mode in all_modes() {
            let low = sine_rms::<LpBiquad>(mode, cutoff_semi, 0.0, 200.0, sr);
            let high = sine_rms::<LpBiquad>(mode, cutoff_semi, 0.0, 8000.0, sr);
            assert!(
                high < low * 0.5,
                "expected lowpass attenuation (mode {mode:?}): low={low} high={high}"
            );
        }
    }

    #[test]
    fn all_modes_survive_resonant_cutoff_sweep() {
        sweep_stays_bounded::<LpBiquad>(&all_modes());
    }

    /// Changing `drive` at runtime switches the coefficient *form* (the SVF form
    /// leaves the output-mix slots `c[4..6]` at zero, so the coupled/lattice kernels
    /// would emit silence on stale coefficients). The crossfade keeps audio flowing and
    /// keeps the transition click-free without touching cutoff/resonance.
    #[test]
    fn drive_change_is_click_free() {
        use crate::poly::{PolyOutput, PolySignal};
        use crate::types::Signal;

        let sr = 48_000.0;
        // A resonant, moving input is where the coefficient-form jump is worst.
        let cutoff = PolySignal::mono(Signal::Volts(0.5));
        let reso = Some(PolySignal::mono(Signal::Volts(4.0)));
        let mut out = PolyOutput::mono(0.0);
        let mut state = FilterModuleState::default();
        let mut lane = vec![FilterLane::default()];
        let sig = |i: usize| (2.0 * std::f32::consts::PI * 220.0 * (i as f32 / sr)).sin() * 5.0;

        let standard = BiquadMode {
            four_pole: true,
            subtype: ST_STANDARD,
        };
        let driven = BiquadMode {
            four_pole: true,
            subtype: ST_DRIVEN,
        };

        // Settle in Standard, measuring the steady-state per-sample step for reference.
        on_patch_update::<LpBiquad>(&mut state, &mut lane, standard);
        let mut prev = 0.0f32;
        let mut steady_max = 0.0f32;
        for i in 0..4000 {
            let input = PolySignal::mono(Signal::Volts(sig(i)));
            run::<LpBiquad>(
                1,
                FilterInputs {
                    input: &input,
                    cutoff: &cutoff,
                    resonance: &reso,
                    mode: standard,
                },
                sr,
                &mut state,
                &mut lane,
                &mut out,
            );
            let y = out.get(0);
            if i > 2000 {
                steady_max = steady_max.max((y - prev).abs());
            }
            prev = y;
        }

        // Form change Standard → Driven: crossfade must keep the step near steady-state.
        on_patch_update::<LpBiquad>(&mut state, &mut lane, driven);
        let n = 4000usize;
        let mut switch_max = 0.0f32;
        let mut sumsq = 0.0f64;
        for i in 0..n {
            let input = PolySignal::mono(Signal::Volts(sig(4000 + i)));
            run::<LpBiquad>(
                1,
                FilterInputs {
                    input: &input,
                    cutoff: &cutoff,
                    resonance: &reso,
                    mode: driven,
                },
                sr,
                &mut state,
                &mut lane,
                &mut out,
            );
            let y = out.get(0);
            switch_max = switch_max.max((y - prev).abs());
            prev = y;
            if i > n / 2 {
                sumsq += (y as f64) * (y as f64);
            }
        }
        let rms = (sumsq / (n / 2) as f64).sqrt();

        assert!(
            rms > 0.1,
            "filter went silent after a drive change: rms={rms}"
        );
        // Without the crossfade this step is a multi-volt coefficient-form jump
        // (≈30× steady-state); it must stay in the same ballpark as normal operation.
        assert!(
            switch_max < steady_max * 4.0 + 0.05,
            "drive change clicked: switch step {switch_max} vs steady {steady_max}"
        );
    }
}
