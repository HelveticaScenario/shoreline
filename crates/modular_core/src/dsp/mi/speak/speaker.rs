//! `LPCSpeechSynthController` on Plaits' word banks: the LPC synth saying a
//! word at 40 frames a second on a trigger, or scanning a bank's frames
//! without one.

use crate::dsp::mi::core::{
    lpc::{CORRECTED_SAMPLE_RATE, DEFAULT_F0, Frame, LpcClock, LpcSynth},
    random::Random,
    tables::{LPC_BANK_WORDS, LPC_WORD_FRAMES, LPC_WORD_STARTS},
    units::semitones_to_ratio,
};
use crate::dsp::utils::rng::hash_bipolar;

use super::Rates;

/// Colours, numbers, letters, the NATO alphabet and synth words.
pub const BANKS: usize = 5;
const FIELDS: usize = 12;

fn frame(index: usize) -> Frame {
    let at = index * FIELDS;
    Frame::from_fields(&LPC_WORD_FRAMES[at..at + FIELDS])
}

/// Where a bank's words sit in `LPC_WORD_STARTS` and its frames in
/// `LPC_WORD_FRAMES`.
struct Bank {
    first_word: usize,
    words: usize,
    first_frame: usize,
    frames: usize,
}

impl Bank {
    /// Where `word` scans to, as Plaits' controller takes it: 0–1 across the
    /// bank. Each word spans half an index either side of its own, so a
    /// whole number sits in the middle of its word.
    #[inline(always)]
    fn scan_address(&self, word: f32) -> f32 {
        let last = (self.words - 1) as f32;
        let word = word.clamp(-0.5, last + 0.5);
        let index = word.round().clamp(0.0, last);
        let start = usize::from(LPC_WORD_STARTS[self.first_word + index as usize]);
        let end = usize::from(LPC_WORD_STARTS[self.first_word + index as usize + 1]);
        let position =
            (start - self.first_frame) as f32 + (word - index + 0.5) * (end - start) as f32;
        (position / (self.frames as f32 - 1.0001)).clamp(0.0, 1.0)
    }

    #[inline(always)]
    fn new(index: usize) -> Self {
        let first_word = usize::from(LPC_BANK_WORDS[index]);
        let end_word = usize::from(LPC_BANK_WORDS[index + 1]);
        let first_frame = usize::from(LPC_WORD_STARTS[first_word]);
        Self {
            first_word,
            words: end_word - first_word,
            first_frame,
            frames: usize::from(LPC_WORD_STARTS[end_word]) - first_frame,
        }
    }
}

/// What the synth's frame is set to while no word plays.
#[derive(Clone, Copy, PartialEq)]
enum Scan {
    /// The bank and word position (as bits) it is scanned to.
    At(usize, u32),
    Silent,
}

/// The controls for a sample, as Plaits' controller takes them.
pub struct Controls {
    pub bank: usize,
    /// Which word, counting from 0: the nearest whole number on a trigger,
    /// and the position a scan sits at, each whole number mid-word.
    pub word: f32,
    /// 0–1.
    pub formant_shift: f32,
    /// How much of each frame's own pitch the voice follows: 0–1.
    pub prosody: f32,
    /// −1 (four times slower) to 1 (four times faster).
    pub speed: f32,
    pub gain: f32,
}

#[derive(Clone, Copy)]
pub struct Speaker {
    /// The noise of unvoiced frames, as Plaits draws its global generator.
    pub(super) random: Random,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// channel, so a patch update never reseeds it.
    pub(super) seeded: bool,
    synth: LpcSynth,
    clock: LpcClock,
    /// The bank whose words a trigger picks from.
    loaded: Option<usize>,
    /// The frame to play next, an index into `LPC_WORD_FRAMES`; `None`
    /// scans.
    playback_frame: Option<usize>,
    /// The word's last frame, which plays on until the next trigger.
    last_frame: usize,
    /// Samples left of the frame playing.
    remaining: usize,
    /// The frame set while no word plays, if it is still set.
    scanned: Option<Scan>,
    /// Whether a patched trigger has yet to say a word since it was patched
    /// or the bank changed; until it does, the voice is silent.
    waiting: bool,
    /// The formant shift (as bits) the values below are for.
    shift: Option<u32>,
    /// Synthesis ticks per sample.
    rate: f32,
    /// The divisor turning a pitch into the synth's pitch shift.
    pitch_unit: f32,
    /// The formant shift's part of the time stretch, in semitones.
    stretch: f32,
    /// The gain of the last sample, which Plaits' gain interpolator steps
    /// from.
    gain: f32,
}

impl Default for Speaker {
    fn default() -> Self {
        Self {
            random: Random::new(0),
            seeded: false,
            synth: LpcSynth::default(),
            clock: LpcClock::default(),
            loaded: None,
            playback_frame: None,
            last_frame: 0,
            remaining: 0,
            scanned: None,
            waiting: true,
            shift: None,
            rate: 0.0,
            pitch_unit: 1.0,
            stretch: 0.0,
            gain: 0.0,
        }
    }
}

impl Speaker {
    /// Seeds the noise for channel `ch` of the instance with seed `base`,
    /// unless it has its seed.
    pub(super) fn seed(&mut self, base: u64, ch: usize) {
        if !self.seeded {
            let unit = (hash_bipolar(base, ch as u64) + 1.0) * 0.5;
            self.random = Random::new((unit * f64::from(u32::MAX)) as u32);
            self.seeded = true;
        }
    }

    /// One sample at `f0` cycles per sample of Plaits' corrected rate: the
    /// excitation and the voice. `trigger` says the word `controls.word`
    /// picks; when it ends, its last frame holds, or with `free_running` the
    /// scan resumes. Without `free_running` the voice is silent until the
    /// first trigger and after a bank change.
    #[inline(always)]
    pub fn step(
        &mut self,
        trigger: bool,
        free_running: bool,
        f0: f32,
        controls: &Controls,
        rates: &Rates,
    ) -> [f32; 2] {
        let formant_shift = controls.formant_shift;
        if self.shift != Some(formant_shift.to_bits()) {
            self.shift = Some(formant_shift.to_bits());
            let rate_ratio = semitones_to_ratio((formant_shift - 0.5) * 36.0);
            self.rate = rate_ratio / rates.lpc_divider;
            self.pitch_unit = rate_ratio * DEFAULT_F0 / CORRECTED_SAMPLE_RATE;
            self.stretch = if formant_shift < 0.4 {
                (formant_shift - 0.4) * -45.0
            } else if formant_shift > 0.6 {
                (formant_shift - 0.6) * -45.0
            } else {
                0.0
            };
        }
        let pitch_shift = f0 / self.pitch_unit;

        let bank = Bank::new(controls.bank);
        if self.loaded != Some(controls.bank) {
            self.loaded = Some(controls.bank);
            self.playback_frame = None;
            self.waiting = true;
        }
        if free_running {
            self.waiting = true;
        }
        if trigger {
            self.waiting = false;
            let last = (bank.words - 1) as f32;
            let word = bank.first_word + controls.word.round().clamp(0.0, last) as usize;
            self.playback_frame = Some(usize::from(LPC_WORD_STARTS[word]));
            self.last_frame = usize::from(LPC_WORD_STARTS[word + 1]) - 1;
            self.remaining = 0;
        }
        match self.playback_frame {
            Some(index) if self.remaining == 0 => {
                let f = frame(index);
                self.synth.play_frame(&f, &f, 0.0);
                self.scanned = None;
                let time_stretch = semitones_to_ratio(-controls.speed * 24.0 + self.stretch);
                self.remaining = (rates.frame_samples * time_stretch) as usize;
                self.playback_frame = if index + 1 < self.last_frame {
                    Some(index + 1)
                } else if free_running {
                    None
                } else {
                    Some(self.last_frame)
                };
            }
            None if self.remaining == 0 && self.waiting && !free_running => {
                if self.scanned != Some(Scan::Silent) {
                    let silent = Frame::from_fields(&[0; FIELDS]);
                    self.synth.play_frame(&silent, &silent, 0.0);
                    self.scanned = Some(Scan::Silent);
                }
            }
            None if self.remaining == 0 => {
                let key = Scan::At(controls.bank, controls.word.to_bits());
                if self.scanned != Some(key) {
                    let position = bank.scan_address(controls.word) * (bank.frames as f32 - 1.0001);
                    let integral = (position as usize).min(bank.frames - 2);
                    let f1 = frame(bank.first_frame + integral);
                    let f2 = frame(bank.first_frame + integral + 1);
                    self.synth.play_frame(&f1, &f2, position - integral as f32);
                    // Between a voiced and an unvoiced frame the pitch moves
                    // on each call, so the scan repeats.
                    self.scanned = (f1.voiced() == f2.voiced()).then_some(key);
                }
            }
            _ => {}
        }
        self.remaining -= self.remaining.min(1);

        let f = self
            .synth
            .glottal_frequency(controls.prosody, pitch_shift)
            .clamp(-0.5, 0.5);
        let [excitation, out] = self
            .clock
            .step(self.rate, f, &mut self.synth, &mut self.random);
        self.gain += controls.gain - self.gain;
        [excitation * self.gain, out * self.gain]
    }
}
