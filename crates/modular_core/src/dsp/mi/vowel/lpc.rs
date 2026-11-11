//! `LPCSpeechSynthController` on Plaits' vowel bank: the LPC synth scanning
//! five vowel frames, a trigger playing one of ten consonant frames.

use crate::dsp::mi::core::{
    lpc::{Frame, LpcClock, LpcSynth},
    random::Random,
    tables::LPC_PHONEMES,
    units::semitones_to_ratio,
};

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        oscillators::FmMode,
        utils::rng::{hash_bipolar, seed_base},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
};

use super::{Channel, Rates, Voice, update_channels};

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
    /// The noise of unvoiced consonants, as Plaits draws its global
    /// generator.
    pub(super) random: Random,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// channel, so a patch update never reseeds it.
    pub(super) seeded: bool,
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
            random: Random::new(0),
            seeded: false,
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
    /// Seeds the noise for channel `ch` of the instance with seed `base`,
    /// unless it has its seed.
    pub(super) fn seed(&mut self, base: u64, ch: usize) {
        if !self.seeded {
            let unit = (hash_bipolar(base, ch as u64) + 1.0) * 0.5;
            self.random = Random::new((unit * f64::from(u32::MAX)) as u32);
            self.seeded = true;
        }
    }
}

impl Voice for LpcVowels {
    /// `vowel` scans a, e, i, o, u; `formant_shift` speeds the synthesis ±18
    /// semitones, and away from the middle stretches the consonant frame a
    /// trigger plays.
    #[inline(always)]
    fn step(
        &mut self,
        trigger: bool,
        f0: f32,
        vowel: f32,
        formant_shift: f32,
        rates: &Rates,
    ) -> (f32, f32) {
        // Plaits reckons LPC pitch at its corrected rate.
        let f0 = f0 * rates.lpc_pitch_scale;
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
        let [excitation, out] = self
            .clock
            .step(self.rate, f, &mut self.synth, &mut self.random);
        (excitation, out)
    }
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct LpcVowelParams {
    #[input("the voice's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "starts a syllable: a consonant of about 25 ms, picked from the knobs and pitch and stretched as `formantShift` leaves the middle. Unpatched, the vowel sustains",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "which vowel: open a (0) through e, i and o to closed u (5)",
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
struct LpcVowelOutputs {
    #[output("output", "the voice", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
    #[output(
        "excitation",
        "the glottal pulses and the noise of unvoiced consonants that the LPC filter shapes",
        type = cv,
        range = (-5.0, 5.0),
    )]
    excitation: PolyOutput,
}

const LPC_VOWEL_DOCS: ModuleDocs = ModuleDocs {
    summary: "Speak-and-spell vowels",
    description: "Mutable Instruments Plaits' LPC vowels: the five vowels \
                  of a speak-and-spell's LPC-10 voice, sung at the pitch, \
                  with a short consonant on `trigger`.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[DocExample {
        title: "Robot syllables",
        code: r#"
                const seq = $p('c3 eb3 g3 <bb2 f3>').c
                $mi.lpcVowel(seq, { trigger: seq }).out()
            "#,
    }],
};

#[module(name = "$mi.lpcVowel", docs = LPC_VOWEL_DOCS, args(freq), has_init, patch_update)]
pub struct LpcVowel {
    outputs: LpcVowelOutputs,
    state: Rates,
    channel_state: Box<[Channel<LpcVowels>]>,
    params: LpcVowelParams,
}

impl LpcVowel {
    fn init(&mut self, sample_rate: f32) {
        self.state = Rates::new(sample_rate);
        for channel in self.channel_state.iter_mut() {
            *channel = Channel::new(&self.state);
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        update_channels!(self, LpcVowelParams);
    }
}

impl crate::types::PatchUpdateHandler for LpcVowel {
    /// Seeds the noise generators of channels that have none yet. Runs here,
    /// not in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same noise.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, channel) in self.channel_state.iter_mut().enumerate() {
            channel.voice.seed(base, ch);
        }
    }
}

message_handlers!(impl LpcVowel {});
