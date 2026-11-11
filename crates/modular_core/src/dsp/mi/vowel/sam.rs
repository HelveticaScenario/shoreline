//! Plaits' `SAMSpeechSynth`, after the Software Automatic Mouth: three sine
//! formants restarted on every glottal pulse and faded across its period,
//! tuned from SAM's table of nine vowels and eight consonants.

use crate::dsp::mi::core::{
    control::Ramp,
    polyblep::{edge_time, next_blep_sample, this_blep_sample},
    sine::sine_raw,
    tables::{FORMANT_AMPLITUDE_LUT, SAM_PHONEMES},
};

use super::Rates;

const FORMANTS: usize = 3;
const VOWELS: usize = 9;
const CONSONANTS: usize = 8;
/// SAM's table rows, the last repeated as a guard.
const ROWS: usize = VOWELS + CONSONANTS + 1;

#[derive(Clone, Copy, Default)]
pub struct SamSynth {
    phase: f32,
    /// Plaits' one-sample `ParameterInterpolator`.
    frequency: Ramp,
    pulse_next_sample: f32,
    pulse_lp: f32,
    formant_phase: [u32; FORMANTS],
    /// Samples left of a consonant, and which consonant.
    consonant: usize,
    consonant_index: f32,
    formant_increment: [u32; FORMANTS],
    formant_amplitude: [f32; FORMANTS],
    /// The phoneme and formant shift (as bits) the formants are tuned for.
    tuned: Option<(u32, u32)>,
    /// The pulse smoothing coefficient, and the speed (as bits) it is for.
    smoothing: f32,
    smoothing_speed: Option<u32>,
}

/// Formant `i` of table row `row`: its pitch in SAM's units (8 Hz) and its
/// level index.
#[inline(always)]
fn entry(row: usize, i: usize) -> (u8, u8) {
    let at = (row.min(ROWS - 1) * FORMANTS + i) * 2;
    (SAM_PHONEMES[at], SAM_PHONEMES[at + 1])
}

impl SamSynth {
    /// `InterpolatePhonemeData`: the formants of `phoneme` (a fractional row)
    /// scaled by `formant_shift` (0–1).
    fn tune(&mut self, phoneme: f32, formant_shift: f32, rates: &Rates) {
        let integral = phoneme as usize;
        let fractional = phoneme - integral as f32;
        let shift = 1.0 + formant_shift * 2.5;
        let scale = 8.0 * shift * 4294967296.0 / rates.sample_rate;
        for i in 0..FORMANTS {
            let (f1, a1) = entry(integral, i);
            let (f2, a2) = entry(integral + 1, i);
            let (f1, f2) = (f32::from(f1), f32::from(f2));
            let f = (f1 + (f2 - f1) * fractional) * scale;
            self.formant_increment[i] = f as u32;
            let a1 = FORMANT_AMPLITUDE_LUT[usize::from(a1) & 15];
            let a2 = FORMANT_AMPLITUDE_LUT[usize::from(a2) & 15];
            self.formant_amplitude[i] = a1 + (a2 - a1) * fractional;
        }
    }

    /// The pulse smoothing per sample at `speed` cycles per sample: Plaits'
    /// `16 f` per 48 kHz sample, as the same time constant at the rate.
    #[inline(always)]
    fn smoothing(&mut self, speed: f32, rates: &Rates) -> f32 {
        if self.smoothing_speed != Some(speed.to_bits()) {
            self.smoothing_speed = Some(speed.to_bits());
            let at_48k = (16.0 * speed * rates.to_48k).min(1.0);
            self.smoothing = if rates.at_48k {
                at_48k
            } else {
                1.0 - (1.0 - at_48k).powf(rates.from_48k)
            };
        }
        self.smoothing
    }

    /// One sample at `f0` cycles per sample: the excitation and the voice.
    /// `consonant` plays one of SAM's consonants for 50 ms, picked from the
    /// knobs and pitch.
    #[inline(always)]
    pub fn step(
        &mut self,
        consonant: bool,
        f0: f32,
        vowel: f32,
        formant_shift: f32,
        rates: &Rates,
    ) -> (f32, f32) {
        let f0 = f0.clamp(-rates.sam_max_frequency, rates.sam_max_frequency);
        if consonant {
            self.consonant = rates.click_samples;
            let r = ((vowel + 3.0 * f0.abs() * rates.to_48k + 7.0 * formant_shift) * 8.0) as i32;
            self.consonant_index = r.rem_euclid(CONSONANTS as i32) as f32;
        }
        self.consonant -= self.consonant.min(1);
        let phoneme = if self.consonant != 0 {
            self.consonant_index + VOWELS as f32
        } else {
            vowel * (VOWELS as f32 - 1.0001)
        };
        let tuning = (phoneme.to_bits(), formant_shift.to_bits());
        if self.tuned != Some(tuning) {
            self.tuned = Some(tuning);
            self.tune(phoneme, formant_shift, rates);
        }

        self.frequency.set(f0, Some(1.0));
        let frequency = self.frequency.next_value();
        let mut this_sample = self.pulse_next_sample;
        let mut next_sample = 0.0;
        self.phase += frequency;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            let t = self.phase / frequency;
            for (phase, &increment) in self.formant_phase.iter_mut().zip(&self.formant_increment) {
                *phase = (t * increment as f32) as u32;
            }
            this_sample -= this_blep_sample(t);
            next_sample -= next_blep_sample(t);
        } else if self.phase < 0.0 {
            let t = edge_time(self.phase, frequency);
            self.phase += 1.0;
            if self.phase >= 1.0 {
                self.phase = 0.0;
            }
            // The formants restart where a forward period ends: as many
            // samples of formant from its start as the phase is from 0.
            let samples = f64::from(self.phase) / f64::from(-frequency);
            for (phase, &increment) in self.formant_phase.iter_mut().zip(&self.formant_increment) {
                *phase = (samples * f64::from(increment)).rem_euclid(4294967296.0) as u32;
            }
            this_sample += this_blep_sample(t);
            next_sample += next_blep_sample(t);
        } else if frequency >= 0.0 {
            for (phase, &increment) in self.formant_phase.iter_mut().zip(&self.formant_increment) {
                *phase = phase.wrapping_add(increment);
            }
        } else {
            for (phase, &increment) in self.formant_phase.iter_mut().zip(&self.formant_increment) {
                *phase = phase.wrapping_sub(increment);
            }
        }
        next_sample += self.phase;
        self.pulse_next_sample = next_sample;

        let d = this_sample - 0.5 - self.pulse_lp;
        self.pulse_lp += self.smoothing(frequency.abs(), rates) * d;

        let mut s = 0.0;
        for i in 0..FORMANTS {
            s += sine_raw(self.formant_phase[i]) * self.formant_amplitude[i];
        }
        (d, s * (1.0 - self.phase))
    }
}
