//! `LPCSpeechSynthController` on Plaits' vowel bank: the LPC synth scanning
//! five vowel frames, a trigger playing one of ten consonant frames.

use crate::dsp::mi::core::{
    lpc::{Frame, LpcClock, LpcSynth},
    random::Random,
    tables::LPC_PHONEMES,
    units::semitones_to_ratio,
};

use super::Rates;

const VOWELS: usize = 5;
const CONSONANTS: usize = 10;
const FIELDS: usize = 12;

/// Plaits' corrected codec rate, which its LPC pitch is reckoned at.
pub const CORRECTED_SAMPLE_RATE: f32 = 47872.34;

/// The pitch every utterance's frames are normalised to.
const DEFAULT_F0: f32 = 100.0;

fn phoneme(index: usize) -> Frame {
    let at = index.min(VOWELS + CONSONANTS - 1) * FIELDS;
    Frame::from_fields(&LPC_PHONEMES[at..at + FIELDS])
}

#[derive(Clone, Copy)]
pub struct LpcVowels {
    synth: LpcSynth,
    clock: LpcClock,
    /// The consonant frame to play next, if any.
    playback_frame: Option<usize>,
    /// Samples left of the frame playing.
    remaining: usize,
    /// The vowel position (as bits) the synth's frame is scanned to.
    scanned: Option<u32>,
    /// The formant shift (as bits) the ratios below are for.
    shift: Option<u32>,
    rate_ratio: f32,
    /// Synthesis ticks per sample.
    rate: f32,
    /// The divisor turning a pitch into the synth's pitch shift.
    pitch_unit: f32,
    time_stretch: f32,
}

impl Default for LpcVowels {
    fn default() -> Self {
        Self {
            synth: LpcSynth::default(),
            clock: LpcClock::default(),
            playback_frame: None,
            remaining: 0,
            scanned: None,
            shift: None,
            rate_ratio: 1.0,
            rate: 0.0,
            pitch_unit: 1.0,
            time_stretch: 1.0,
        }
    }
}

impl LpcVowels {
    /// One sample at `f0` cycles per sample of Plaits' corrected rate: the
    /// excitation and the voice. `vowel` scans a, e, i, o, u; `formant_shift`
    /// (0–1) speeds the synthesis ±18 semitones, and away from the middle
    /// stretches the consonants a `trigger` plays.
    #[inline(always)]
    pub fn step(
        &mut self,
        trigger: bool,
        f0: f32,
        vowel: f32,
        formant_shift: f32,
        random: &mut Random,
        rates: &Rates,
    ) -> [f32; 2] {
        if self.shift != Some(formant_shift.to_bits()) {
            self.shift = Some(formant_shift.to_bits());
            self.rate_ratio = semitones_to_ratio((formant_shift - 0.5) * 36.0);
            self.rate = self.rate_ratio / rates.lpc_divider;
            self.pitch_unit = self.rate_ratio * DEFAULT_F0 / CORRECTED_SAMPLE_RATE;
            self.time_stretch = semitones_to_ratio(if formant_shift < 0.4 {
                (formant_shift - 0.4) * -45.0
            } else if formant_shift > 0.6 {
                (formant_shift - 0.6) * -45.0
            } else {
                0.0
            });
        }
        let pitch_shift = f0 / self.pitch_unit;

        if trigger {
            let r = ((vowel + 3.0 * formant_shift + 7.0 * f0.abs()) * 8.0) as i32;
            self.playback_frame = Some(VOWELS + r.rem_euclid(CONSONANTS as i32) as usize);
            self.remaining = 0;
        }
        match self.playback_frame {
            None if self.remaining == 0 => {
                if self.scanned != Some(vowel.to_bits()) {
                    self.scanned = Some(vowel.to_bits());
                    let frame = vowel * (VOWELS as f32 - 1.0001);
                    let integral = (frame as usize).min(VOWELS - 2);
                    self.synth.play_frame(
                        &phoneme(integral),
                        &phoneme(integral + 1),
                        frame - integral as f32,
                    );
                }
            }
            playback_frame => {
                if self.remaining == 0 {
                    if let Some(index) = playback_frame {
                        let frame = phoneme(index);
                        self.synth.play_frame(&frame, &frame, 0.0);
                        self.scanned = None;
                    }
                    self.remaining = (rates.frame_samples * self.time_stretch) as usize;
                    self.playback_frame = None;
                }
                self.remaining -= self.remaining.min(1);
            }
        }

        let f = (DEFAULT_F0 / 8000.0 * pitch_shift).clamp(-0.5, 0.5);
        self.clock.step(self.rate, f, &mut self.synth, random)
    }
}
