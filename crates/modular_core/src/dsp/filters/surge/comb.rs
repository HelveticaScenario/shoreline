//! `$unstable.filter.comb` — Surge XT's comb filter (`fut_comb_pos`/`fut_comb_neg`):
//! a sinc-interpolated feedback delay line tuned to the cutoff pitch, with positive
//! or negative feedback and a 50%/100% wet mix.
//!
//! Ported from https://github.com/surge-synthesizer/sst-filters (GPL-3.0).

use deserr::Deserr;
use schemars::JsonSchema;

use super::coeffs::note_to_hz;
use super::fastmath::softclip;
use super::filter_core::{Filter, N_COEFFS, N_REGISTERS};
use super::sinc::{self, FIRIPOL_N, FIROFFSET};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Delay-line length (Surge `MAX_FB_COMB` with the default ×2 extension factor —
/// Surge XT's longer-ringing combs). Must be a power of two.
const COMB_SIZE: usize = 4096;

/// Per-channel delay line + write position. `Default` allocates (module
/// construction runs on the main thread) and primes the sinc table. The buffer
/// carries [`FIRIPOL_N`] extra samples mirroring the start so the interpolator's
/// 12-tap read never wraps mid-kernel.
pub struct CombExtra {
    delay: Box<[f32]>,
    wp: usize,
}

impl Default for CombExtra {
    fn default() -> Self {
        sinc::prime();
        Self {
            delay: vec![0.0f32; COMB_SIZE + FIRIPOL_N].into_boxed_slice(),
            wp: 0,
        }
    }
}

/// Surge subtype: bit 0 = 100% wet, bit 1 = negative feedback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CombMode {
    pub subtype: i32,
}

/// Feedback polarity: positive resonates at harmonics of the cutoff pitch,
/// negative at the half-shifted series.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum CombPolarity {
    /// rings at the cutoff pitch and its harmonics, bright and metallic
    #[default]
    Pos,
    /// rings an octave lower on odd harmonics only, hollow and reedy
    Neg,
}

/// Wet mix: Surge XT's "50%" / "100%" comb subtypes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum CombMix {
    /// dry and delayed signal together, a flanger-like comb of peaks and notches
    #[default]
    Half,
    /// the delayed, ringing signal alone
    Full,
}

/// Comb kernel. The feedback path soft-clips per sample → 2× oversampled (matching
/// Surge XT's oversampled voice rate, where this buffer length has the same ring time).
#[derive(Clone, Copy, Default)]
pub struct Comb;

impl Filter for Comb {
    type Mode = CombMode;
    type Extra = CombExtra;

    /// Surge `Coeff_COMB`: the delay in samples (one period of the cutoff pitch),
    /// signed feedback, and the wet/dry mix. Uses the correctly-tuned delay (Surge XT's
    /// modern `correctlyTuneCombFilter` behavior — no legacy `FIRoffset` shift).
    fn coeffs(mode: CombMode, freq_semi: f32, reso: f32, rate: f32) -> [f32; N_COEFFS] {
        let dtime =
            (rate / note_to_hz(freq_semi)).clamp(FIRIPOL_N as f32, (COMB_SIZE - FIRIPOL_N) as f32);
        let sign = if mode.subtype & 2 != 0 { -1.0 } else { 1.0 };

        let mut c = [0.0f32; N_COEFFS];
        c[0] = dtime;
        c[1] = sign * reso.clamp(0.0, 1.0);
        c[2] = if mode.subtype & 1 != 0 { 0.0 } else { 0.5 }; // dry mix
        c[3] = 1.0 - c[2]; // wet mix
        c
    }

    fn process(
        _mode: CombMode,
        x: f32,
        c: &mut [f32; N_COEFFS],
        dc: &[f32; N_COEFFS],
        r: &mut [f32; N_REGISTERS],
        extra: &mut CombExtra,
    ) -> f32 {
        let _ = r; // all state lives in the delay line
        // All four slots advance so a polarity/mix change glides through the
        // coefficients (mode changes re-target rather than crossfade — the delay
        // line is shared between any two modes).
        for i in 0..4 {
            c[i] += dc[i];
        }

        // Split the glided delay time into integer samples + sinc phase.
        let e = (c[0] * 256.0).round_ties_even() as i32;
        let dt = (e >> 8) as isize;
        let se = (255 - (e & 255)) as usize;

        let rp =
            ((extra.wp as isize - dt - FIROFFSET as isize) & (COMB_SIZE as isize - 1)) as usize;

        let taps = sinc::taps(se);
        let mut db_read = 0.0f32;
        for (i, tap) in taps.iter().enumerate() {
            db_read += extra.delay[rp + i] * tap;
        }

        let d = softclip(x + db_read * c[1]);
        extra.delay[extra.wp] = d;
        if extra.wp < FIRIPOL_N {
            extra.delay[extra.wp + COMB_SIZE] = d;
        }
        extra.wp = (extra.wp + 1) & (COMB_SIZE - 1);

        c[3] * db_read + c[2] * x
    }

    fn oversample(_mode: CombMode) -> bool {
        true
    }

    fn crossfade_on_mode_change() -> bool {
        false
    }
}

const COMB_DOCS: ModuleDocs = ModuleDocs {
    summary: "Comb filter",
    description: "A short feedback delay tuned to the cutoff that rings at that pitch and its \
                  harmonics, for metallic, plucked-string and flanging tones. `'neg'` polarity \
                  rings an octave lower and hollower.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[DocExample {
        title: "Plucked strings from noise bursts",
        code: r#"
            const burst = $noise('white').amp(
              $perc($clock.o.beatTrigger, { decay: 0.02 }),
            )
            const notes = $p('c3 eb3 g3 bb3').c
            $unstable.filter
              .comb(burst, notes, 4.5, { mix: 'full' })
              .out()
        "#,
    }],
};

filter_module! {
    docs = COMB_DOCS,
    name = "$unstable.filter.comb", ident = CombFilter, kernel = Comb,
    output_doc = "comb output",
    cutoff_doc = "pitch the comb rings at",
    params = {
        #[input("feedback polarity: which harmonic series rings", default = "pos")]
        polarity: CombPolarity,
        #[input("how much of the comb is heard against the dry signal", default = "half")]
        mix: CombMix,
    },
    mode = |p| {
        let polarity = if matches!(p.polarity, CombPolarity::Neg) { 2 } else { 0 };
        let wet = if matches!(p.mix, CombMix::Full) { 1 } else { 0 };
        CombMode { subtype: polarity | wet }
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::filters::surge::filter_core::test_util::{sine_rms_amp, sweep_stays_bounded};

    const SR: f32 = 96_000.0; // kernel run rate is 2× the 48 kHz engine rate

    /// Small-signal probe amplitude: the write path always soft-clips, so full-scale
    /// input compresses the recirculating signal and flattens the resonant peaks.
    const AMP: f32 = 0.05;

    /// 500 Hz comb pitch expressed in semitones above A440 (delay = 192 samples).
    fn pitch_semi() -> f32 {
        12.0 * (500.0f32 / 440.0).log2()
    }

    fn rms(subtype: i32, reso: f32, freq: f32) -> f32 {
        sine_rms_amp::<Comb>(CombMode { subtype }, pitch_semi(), reso, freq, SR, AMP)
    }

    #[test]
    fn positive_comb_resonates_at_harmonics() {
        // Peak at the comb pitch, trough halfway between harmonics.
        let on_peak = rms(0, 0.9, 500.0);
        let off_peak = rms(0, 0.9, 750.0);
        assert!(
            on_peak > off_peak * 2.0,
            "expected harmonic resonance: on={on_peak} off={off_peak}"
        );
    }

    #[test]
    fn negative_comb_shifts_the_series() {
        // Negative feedback resonates at the half-shifted series (250 Hz, 750 Hz…)
        // and dips at the pitch itself.
        let on_peak = rms(2, 0.9, 250.0);
        let off_peak = rms(2, 0.9, 500.0);
        assert!(
            on_peak > off_peak * 2.0,
            "expected shifted resonance: on={on_peak} off={off_peak}"
        );
    }

    #[test]
    fn mix_50_notches_and_mix_100_is_flat() {
        // At zero feedback: 50% mix is a feedforward comb (nulls at odd half
        // harmonics); 100% mix is a pure delay (flat magnitude).
        let notched = rms(0, 0.0, 750.0);
        let flat = rms(1, 0.0, 750.0);
        let unit = AMP * std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            notched < unit * 0.3,
            "expected a notch at 1.5× pitch: {notched}"
        );
        assert!(
            (flat - unit).abs() < unit * 0.1,
            "expected flat magnitude at 100% mix: {flat} vs {unit}"
        );
    }

    #[test]
    fn all_modes_survive_resonant_cutoff_sweep() {
        let modes: Vec<CombMode> = (0..4).map(|subtype| CombMode { subtype }).collect();
        sweep_stays_bounded::<Comb>(&modes);
    }
}
