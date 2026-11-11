//! Plaits' `NaiveSpeechSynth`: a band-limited impulse train through five
//! band-pass formant filters, their tunings and levels interpolated across a
//! table of five vowels in five vocal registers.

use crate::dsp::mi::core::{
    control::Ramp,
    filter::{Svf, tan_dirty},
    polyblep::{edge_time, next_blep_sample, this_blep_sample},
    tables::NAIVE_PHONEMES,
    units::semitones_to_ratio,
};

use super::Rates;

const FORMANTS: usize = 5;
const PHONEMES: usize = 5;
const REGISTERS: usize = 5;

/// Plaits' `a0`: 55 Hz in cycles per sample at its corrected codec rate.
const A0: f32 = (440.0 / 8.0) / 47872.34;

/// `plaits::Oscillator` with the impulse-train shape: a band-limited saw,
/// differentiated and smoothed.
#[derive(Clone, Copy)]
struct ImpulseTrain {
    phase: f32,
    next_sample: f32,
    lp: f32,
    hp: f32,
    /// Plaits' one-sample `ParameterInterpolator`, kept so the train matches
    /// Plaits bit for bit.
    frequency: Ramp,
}

impl Default for ImpulseTrain {
    fn default() -> Self {
        let mut frequency = Ramp::default();
        frequency.set(0.001, None);
        Self {
            phase: 0.5,
            next_sample: 0.0,
            lp: 1.0,
            hp: 0.0,
            frequency,
        }
    }
}

impl ImpulseTrain {
    /// One sample at `frequency` cycles per sample (within ±0.5). Backward,
    /// the saw falls and its derivative is negated, so the pulses keep their
    /// polarity.
    #[inline(always)]
    fn step(&mut self, frequency: f32, rates: &Rates) -> f32 {
        self.frequency.set(frequency, Some(1.0));
        let frequency = self.frequency.next_value();
        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        self.phase += frequency;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            let t = edge_time(self.phase, frequency);
            this_sample -= this_blep_sample(t);
            next_sample -= next_blep_sample(t);
        } else if self.phase < 0.0 {
            let t = edge_time(self.phase, frequency);
            self.phase += 1.0;
            if self.phase >= 1.0 {
                self.phase = 0.0;
            }
            this_sample += this_blep_sample(t);
            next_sample += next_blep_sample(t);
        }
        next_sample += self.phase;
        self.next_sample = next_sample;

        let slope = self.hp - this_sample;
        let slope = if frequency < 0.0 { -slope } else { slope };
        self.lp += rates.impulse_smoothing * (slope * rates.to_48k - self.lp);
        self.hp = this_sample;
        4.0 * self.lp
    }
}

#[derive(Clone, Copy, Default)]
pub struct NaiveSynth {
    pulse: ImpulseTrain,
    coloration: Svf,
    formants: [Svf; FORMANTS],
    amplitudes: [f32; FORMANTS],
    /// Samples left of a click.
    click: usize,
    /// The phoneme, register (as bits) and click the formants are tuned for.
    tuned: Option<(u32, u32, bool)>,
}

/// Formant `i` of `phoneme` in `register`: its pitch in Plaits' table units
/// and its level out of 255.
#[inline(always)]
fn entry(phoneme: usize, register: usize, i: usize) -> (u8, u8) {
    let at = ((phoneme * REGISTERS + register) * FORMANTS + i) * 2;
    (NAIVE_PHONEMES[at], NAIVE_PHONEMES[at + 1])
}

impl NaiveSynth {
    pub fn new(rates: &Rates) -> Self {
        let mut synth = Self::default();
        synth.coloration.set_g_q(rates.coloration, 0.5);
        synth
    }

    /// Retunes the formants for `phoneme` and `register` (0–1); a click
    /// drops the first an octave.
    fn tune(&mut self, phoneme: f32, register: f32, clicking: bool, rates: &Rates) {
        let p = phoneme * (PHONEMES as f32 - 1.001);
        let r = register * (REGISTERS as f32 - 1.001);
        let p_integral = (p as usize).min(PHONEMES - 2);
        let p_fractional = p - p_integral as f32;
        let r_integral = (r as usize).min(REGISTERS - 2);
        let r_fractional = r - r_integral as f32;
        for i in 0..FORMANTS {
            let (f00, a00) = entry(p_integral, r_integral, i);
            let (f01, a01) = entry(p_integral, r_integral + 1, i);
            let (f10, a10) = entry(p_integral + 1, r_integral, i);
            let (f11, a11) = entry(p_integral + 1, r_integral + 1, i);
            let lerp =
                |a: u8, b: u8, x: f32| f32::from(a) + (i32::from(b) - i32::from(a)) as f32 * x;
            let f0 = lerp(f00, f01, r_fractional);
            let f1 = lerp(f10, f11, r_fractional);
            let f = (f0 + (f1 - f0) * p_fractional).min(160.0);
            let a0 = lerp(a00, a01, r_fractional);
            let a1 = lerp(a10, a11, r_fractional);
            self.amplitudes[i] = (a0 + (a1 - a0) * p_fractional) / 256.0;

            let mut f = A0 * semitones_to_ratio(f - 33.0) * rates.from_48k;
            if clicking && i == 0 {
                f *= 0.5;
            }
            self.formants[i].set_g_q(tan_dirty(f.min(0.49)), 20.0);
        }
    }

    /// One sample at `f0` cycles per sample: the excitation and the voice.
    /// `click` drops the pitch an octave for 50 ms.
    #[inline(always)]
    pub fn step(
        &mut self,
        click: bool,
        f0: f32,
        phoneme: f32,
        register: f32,
        rates: &Rates,
    ) -> (f32, f32) {
        if click {
            self.click = rates.click_samples;
        }
        self.click -= self.click.min(1);
        let clicking = self.click != 0;
        let f0 = if clicking { f0 * 0.5 } else { f0 };

        let excitation = self
            .pulse
            .step(f0.clamp(-rates.max_frequency, rates.max_frequency), rates);
        let excitation = self.coloration.band_pass(excitation) * 4.0;

        let tuning = (phoneme.to_bits(), register.to_bits(), clicking);
        if self.tuned != Some(tuning) {
            self.tuned = Some(tuning);
            self.tune(phoneme, register, clicking, rates);
        }
        let mut out = 0.0;
        for (formant, &amplitude) in self.formants.iter_mut().zip(&self.amplitudes) {
            out += amplitude * formant.band_pass(excitation);
        }
        (excitation, out)
    }
}
