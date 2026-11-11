//! `$mi.speak`: Plaits' LPC speech synth saying the words of its five word
//! banks, as its speech engine runs them, through its limiter.

mod speaker;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            limiter::{Limiter, LimiterRates},
            lpc::{CORRECTED_SAMPLE_RATE, SYNTHESIS_RATE},
            phase::limit_frequency,
        },
        oscillators::{FmMode, apply_fm},
        utils::{SchmittTrigger, rng::seed_base},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use super::vowel::{EXCITATION_GAIN, OUTPUT_GAIN, PRE_GAIN};
use speaker::{BANKS, Controls, Speaker};

/// Rate conversions for the engine's rate, stated against Plaits' 48 kHz.
#[derive(Default)]
pub struct Rates {
    sample_rate: f32,
    inv_sample_rate: f32,
    /// Engine samples per LPC synthesis tick at a formant ratio of 1.
    lpc_divider: f32,
    /// Cycles per sample times this is cycles per sample of Plaits'
    /// corrected rate, the unit its LPC pitch is reckoned in.
    lpc_pitch_scale: f32,
    /// A frame's 25 ms.
    frame_samples: f32,
    limiter: LimiterRates,
}

impl Rates {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            lpc_divider: sample_rate / SYNTHESIS_RATE,
            lpc_pitch_scale: (f64::from(sample_rate) / f64::from(CORRECTED_SAMPLE_RATE)) as f32,
            frame_samples: sample_rate / 40.0,
            limiter: LimiterRates::new(sample_rate),
        }
    }
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SpeakParams {
    #[input("the voice's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "says the word `word` picks, then falls silent until the next trigger; silent too until the first trigger and after `bank` changes. Unpatched, the voice holds the sound `word` scans to",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "which word list: colours (0), the numbers zero to ten (1), the letters (2), the NATO alphabet (3) or synthesizer words (4)",
        type = control,
        default = 0.0,
        range = (0.0, 4.0),
    )]
    #[deserr(default)]
    bank: Option<PolySignal>,
    #[input(
        "which word, counting from 0: in the numbers bank, 3 says \"three\"; past a bank's last word, its last. Unpatched trigger, scrubs through the words, each whole number in the middle of its word",
        type = control,
        default = 0.0,
        range = (0.0, 25.0),
    )]
    #[deserr(default)]
    word: Option<PolySignal>,
    #[input(
        "the size of the throat: the formants rise from a large, deep voice (0) to a small, high one (5); towards the ends a deep voice speaks slower, a high one faster",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    formant_shift: Option<PolySignal>,
    #[input(
        "how far the voice follows the recorded intonation of each word: a monotone at the pitch (0) to the word's own rise and fall around it (5)",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    prosody: Option<PolySignal>,
    #[input(
        "how fast a triggered word is said: four times slower (0) to four times faster (5) than recorded",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    speed: Option<PolySignal>,
    #[input(
        "how loud each triggered word is; unpatched trigger, full level",
        type = control,
        default = 4.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
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
struct SpeakOutputs {
    #[output("output", "the voice", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
    #[output(
        "excitation",
        "the glottal pulses and the noise of unvoiced sounds that the LPC filter shapes",
        type = cv,
        range = (-5.0, 5.0),
    )]
    excitation: PolyOutput,
}

const SPEAK_DOCS: ModuleDocs = ModuleDocs {
    summary: "Speak-and-spell words",
    description: "Mutable Instruments Plaits' talking voice: a speak-and-spell's \
                  LPC-10 synth saying colours, numbers, letters and synth words \
                  at the pitch you play.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Hello",
            code: r#"
                const word = $cycle($p('7 4 11 11 14'))
                const trigger = word.o.trig
                $mi.speak('c3', { trigger, bank: 2, word }).out()
            "#,
        },
        DocExample {
            title: "Scanned machine voice",
            code: r#"
                const scan = $sine('0.05hz').range(0, 21)
                $mi.speak('c2', { bank: 4, word: scan, prosody: 5 }).out()
            "#,
        },
    ],
};

/// The knobs for a sample, in volts, with their glide marks: the word,
/// formant shift, prosody, speed and accent.
pub struct Knobs {
    pub bank: f32,
    pub smooth: [(f32, GlideMark); 5],
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

/// A selector in volts as the nearest of `count` indices; `None` when NaN.
fn select(volts: f32, count: usize) -> Option<usize> {
    (!volts.is_nan()).then(|| volts.round().clamp(0.0, (count - 1) as f32) as usize)
}

/// One channel: Plaits' word-bank speech, then its limiter.
pub struct Channel {
    speaker: Speaker,
    limiter: Limiter,
    trigger: SchmittTrigger,
    glides: [SwapGlide; 5],
    /// The last finite value of each smoothed knob, which a NaN or infinite
    /// one holds.
    held: [f32; 5],
    /// The last bank a finite `bank` selected.
    bank: usize,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            speaker: Speaker::default(),
            limiter: Limiter::default(),
            trigger: SchmittTrigger::default(),
            glides: Default::default(),
            held: [
                SpeakParams::WORD_DEFAULT,
                SpeakParams::FORMANT_SHIFT_DEFAULT,
                SpeakParams::PROSODY_DEFAULT,
                SpeakParams::SPEED_DEFAULT,
                SpeakParams::ACCENT_DEFAULT,
            ],
            bank: 0,
        }
    }
}

impl Channel {
    /// One sample at `f0` cycles per sample: the limited voice and the
    /// excitation, before the output gains. A rising `trigger` says a word;
    /// `None` is an unpatched trigger.
    #[inline(always)]
    fn step(&mut self, f0: f32, knobs: Knobs, trigger: Option<f32>, rates: &Rates) -> (f32, f32) {
        let triggered = match trigger {
            Some(volts) => self.trigger.process(volts),
            None => {
                // A trigger patched again starts from low.
                self.trigger = SchmittTrigger::default();
                false
            }
        };
        for ((held, glide), (volts, mark)) in
            self.held.iter_mut().zip(&mut self.glides).zip(knobs.smooth)
        {
            let value = glide.follow(volts, mark, rates.sample_rate);
            if value.is_finite() {
                *held = value;
            }
        }
        if let Some(bank) = select(knobs.bank, BANKS) {
            self.bank = bank;
        }
        let [word, formant_shift, prosody, speed, accent] = self.held;
        // A patched trigger picks a word, so `word` takes effect unglided.
        let word = match (trigger, knobs.smooth[0].0) {
            (Some(_), volts) if volts.is_finite() => volts,
            _ => word,
        };
        let [formant_shift, prosody, speed, accent] =
            [formant_shift, prosody, speed, accent].map(knob);
        let controls = Controls {
            bank: self.bank,
            word,
            formant_shift,
            prosody,
            speed: speed * 2.0 - 1.0,
            // Plaits applies the accent only to triggered words.
            gain: if trigger.is_some() { accent } else { 1.0 },
        };
        let f0 = limit_frequency(f0, 0.5) * rates.lpc_pitch_scale;
        let [excitation, out] =
            self.speaker
                .step(triggered, trigger.is_none(), f0, &controls, rates);
        (
            self.limiter.process(out * PRE_GAIN, &rates.limiter),
            excitation,
        )
    }

    /// One sample through the output stage: the voice and the excitation, in
    /// volts.
    #[inline(always)]
    fn output(&mut self, f0: f32, knobs: Knobs, trigger: Option<f32>, rates: &Rates) -> (f32, f32) {
        let (out, excitation) = self.step(f0, knobs, trigger, rates);
        (
            (OUTPUT_GAIN * out).clamp(-5.0, 5.0),
            (EXCITATION_GAIN * excitation).clamp(-5.0, 5.0),
        )
    }
}

#[module(name = "$mi.speak", docs = SPEAK_DOCS, args(freq), has_init, patch_update)]
pub struct Speak {
    outputs: SpeakOutputs,
    state: Rates,
    channel_state: Box<[Channel]>,
    params: SpeakParams,
}

impl Speak {
    fn init(&mut self, sample_rate: f32) {
        self.state = Rates::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let rates = &self.state;
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, SpeakParams::FM_DEFAULT),
                p.fm_mode,
            ) * rates.inv_sample_rate;
            let trigger = p.trigger.as_ref().map(|t| t.get_value(ch));
            let knob = |signal: &Option<PolySignal>, default| {
                (signal.value_or(ch, default), signal.glide_mark(ch))
            };
            let knobs = Knobs {
                bank: p.bank.value_or(ch, SpeakParams::BANK_DEFAULT),
                smooth: [
                    knob(&p.word, SpeakParams::WORD_DEFAULT),
                    knob(&p.formant_shift, SpeakParams::FORMANT_SHIFT_DEFAULT),
                    knob(&p.prosody, SpeakParams::PROSODY_DEFAULT),
                    knob(&p.speed, SpeakParams::SPEED_DEFAULT),
                    knob(&p.accent, SpeakParams::ACCENT_DEFAULT),
                ],
            };
            let (out, excitation) = channel.output(f0, knobs, trigger, rates);
            self.outputs.sample.set(ch, out);
            self.outputs.excitation.set(ch, excitation);
        }
    }
}

impl crate::types::PatchUpdateHandler for Speak {
    /// Seeds the noise generators of channels that have none yet. Runs here,
    /// not in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same noise.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, channel) in self.channel_state.iter_mut().enumerate() {
            channel.speaker.seed(base, ch);
        }
    }
}

message_handlers!(impl Speak {});

#[cfg(test)]
mod tests;
