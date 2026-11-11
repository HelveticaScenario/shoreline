//! Plaits' `SAMSpeechSynth`, after the Software Automatic Mouth: three sine
//! formants restarted on every glottal pulse and faded across its period,
//! tuned from SAM's table of nine vowels and eight consonants.

use crate::dsp::mi::core::{
    control::Ramp,
    polyblep::{edge_time, next_blep_sample, this_blep_sample},
    sine::sine_raw,
    tables::{FORMANT_AMPLITUDE_LUT, SAM_PHONEMES},
};

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::oscillators::FmMode,
    poly::{PolyOutput, PolySignal, PolySignalExt},
};

use super::{Channel, Rates, Voice, update_channels};

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
}

impl Voice for SamSynth {
    /// A trigger plays one of SAM's consonants for 50 ms, picked from the
    /// knobs and pitch.
    #[inline(always)]
    fn step(
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

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SamVowelParams {
    #[input("the voice's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "starts a syllable: one of SAM's consonants for 50 ms, picked from the knobs and pitch. Unpatched, the vowel sustains",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "sweeps SAM's nine vowels, roughly open (0) to closed (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    vowel: Option<PolySignal>,
    #[input(
        "the size of the throat: the formants rise from a large, deep voice (0) to a small, high one (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    formant_shift: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs the glottis backward",
        type = audio,
    )]
    #[deserr(default)]
    fm: Option<PolySignal>,
    #[input("how `fm` bends the frequency")]
    #[serde(default)]
    #[deserr(default)]
    fm_mode: FmMode,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SamVowelOutputs {
    #[output("output", "the voice", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
    #[output(
        "excitation",
        "the glottal pulses the formants restart on",
        type = cv,
        range = (-5.0, 5.0),
    )]
    excitation: PolyOutput,
}

const SAM_VOWEL_DOCS: ModuleDocs = ModuleDocs {
    summary: "Vowels from SAM",
    description: "Mutable Instruments Plaits' take on the Software \
                  Automatic Mouth, a 1982 home-computer speech \
                  synthesizer: three sine formants restarted on every \
                  glottal pulse, for a buzzy, retro voice with consonants \
                  on `trigger`.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[DocExample {
        title: "Retro syllables",
        code: r#"
                const seq = $p('c3 eb3 g3 <bb2 f3>').c
                const vowel = $sine('0.2hz').range(0, 5)
                $mi.samVowel(seq, { trigger: seq, vowel }).out()
            "#,
    }],
};

#[module(name = "$mi.samVowel", docs = SAM_VOWEL_DOCS, args(freq), has_init)]
pub struct SamVowel {
    outputs: SamVowelOutputs,
    state: Rates,
    channel_state: Box<[Channel<SamSynth>]>,
    params: SamVowelParams,
}

impl SamVowel {
    fn init(&mut self, sample_rate: f32) {
        self.state = Rates::new(sample_rate);
        for channel in self.channel_state.iter_mut() {
            *channel = Channel::new(&self.state);
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        update_channels!(self, SamVowelParams);
    }
}

message_handlers!(impl SamVowel {});
