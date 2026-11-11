mod lpc;
mod naive;
mod sam;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            filter::tan_dirty,
            limiter::{Limiter, LimiterRates},
            lpc::SYNTHESIS_RATE,
            phase::{limit_frequency, max_frequency},
            random::Random,
        },
        oscillators::{FmMode, apply_fm},
        utils::{
            SchmittTrigger,
            rng::{hash_bipolar, seed_base},
        },
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use lpc::{CORRECTED_SAMPLE_RATE, LpcVowels};
use naive::NaiveSynth;
use sam::SamSynth;

/// Plaits' gain into its limiter and its excitation gain, at ±5 V full
/// scale; both outputs clip at ±5 V as Plaits does.
const PRE_GAIN: f32 = 0.7;
const OUTPUT_GAIN: f32 = -5.0;
const EXCITATION_GAIN: f32 = -0.8 * 5.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct VowelParams {
    #[input("the voice's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "starts a syllable: a short consonant before the vowel, picked from the knobs and pitch, or a brief drop in pitch on the formant-filter model. Unpatched, the vowel sustains",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "the synthesis, crossfading from a formant-filter voice (0) to SAM's buzzy sine formants (2.5) to an LPC voice like a speak-and-spell (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    model: Option<PolySignal>,
    #[input(
        "which vowel: open a (0) through e, i and o to closed u (5) on the formant-filter and LPC voices; SAM sweeps its own nine vowels, roughly open to closed",
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
struct VowelOutputs {
    #[output("output", "the voice", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
    #[output(
        "excitation",
        "the glottal source the formants shape: pulses, a buzz, or the LPC voice's pulses and noise",
        type = cv,
        range = (-5.0, 5.0),
    )]
    excitation: PolyOutput,
}

const VOWEL_DOCS: ModuleDocs = ModuleDocs {
    summary: "Sung and spoken vowels",
    description: "Mutable Instruments Plaits' vowel synthesis: three \
                  generations of speech synthesizer, a formant filter bank, \
                  SAM and a speak-and-spell's LPC, singing vowels at the \
                  pitch, with consonants on `trigger`.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Morphing vowels",
            code: r#"
                const vowel = $sine('0.15hz').range(0, 5)
                $mi.vowel('c2', { vowel, model: 0 }).out()
            "#,
        },
        DocExample {
            title: "Robot syllables",
            code: r#"
                const seq = $p('c3 eb3 g3 <bb2 f3>').c
                $mi.vowel(seq, { trigger: seq, model: 5 }).out()
            "#,
        },
    ],
};

#[module(name = "$mi.vowel", docs = VOWEL_DOCS, args(freq), has_init, patch_update)]
pub struct Vowel {
    outputs: VowelOutputs,
    state: Rates,
    channel_state: Box<[Channel]>,
    params: VowelParams,
}

/// Rate conversions for the engine's rate, stated against Plaits' 48 kHz.
#[derive(Default)]
struct Rates {
    sample_rate: f32,
    inv_sample_rate: f32,
    /// Cycles per sample times this is cycles per 48 kHz sample.
    to_48k: f32,
    from_48k: f32,
    at_48k: bool,
    /// The impulse train's highest frequency, in cycles per sample.
    max_frequency: f32,
    /// SAM's highest glottal frequency, 3 kHz, in cycles per sample.
    sam_max_frequency: f32,
    /// A click's or SAM consonant's 50 ms.
    click_samples: usize,
    /// The impulse train's smoothing per sample, and its pulse height, which
    /// keeps the harmonics' level at any rate.
    impulse_smoothing: f32,
    /// `tan(π f)` of the 800 Hz band-pass that colours the impulses.
    coloration: f32,
    /// Engine samples per LPC synthesis tick at a formant ratio of 1.
    lpc_divider: f32,
    /// Cycles per sample times this is cycles per sample of Plaits'
    /// corrected rate, the unit its LPC pitch is reckoned in.
    lpc_pitch_scale: f32,
    /// An LPC frame's 25 ms.
    frame_samples: f32,
    limiter: LimiterRates,
}

impl Rates {
    fn new(sample_rate: f32) -> Self {
        let from_48k = 48000.0 / f64::from(sample_rate);
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            from_48k: from_48k as f32,
            at_48k: sample_rate == 48000.0,
            max_frequency: max_frequency(sample_rate).min(0.5),
            sam_max_frequency: 3000.0 / sample_rate,
            click_samples: (sample_rate * 0.05) as usize,
            impulse_smoothing: (1.0 - 0.75f64.powf(from_48k)) as f32,
            coloration: tan_dirty(800.0 / sample_rate),
            lpc_divider: sample_rate / SYNTHESIS_RATE,
            lpc_pitch_scale: (f64::from(sample_rate) / f64::from(CORRECTED_SAMPLE_RATE)) as f32,
            frame_samples: sample_rate / 40.0,
            limiter: LimiterRates::new(sample_rate),
        }
    }
}

/// The knobs for a sample, in volts, with their glide marks.
struct Knobs {
    model: (f32, GlideMark),
    vowel: (f32, GlideMark),
    formant_shift: (f32, GlideMark),
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

/// Plaits' crossfade curve: smoothstep applied twice.
#[inline(always)]
fn crossfade(blend: f32) -> f32 {
    let blend = blend * (blend * (3.0 - 2.0 * blend));
    blend * (blend * (3.0 - 2.0 * blend))
}

/// One channel: Plaits' `SpeechEngine` on its vowel path, a sample at a time.
struct Channel {
    naive: NaiveSynth,
    sam: SamSynth,
    lpc: LpcVowels,
    limiter: Limiter,
    /// The LPC synth's noise, as Plaits draws its global generator.
    random: Random,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// channel, so a patch update never reseeds it.
    seeded: bool,
    trigger: SchmittTrigger,
    model: SwapGlide,
    vowel: SwapGlide,
    formant_shift: SwapGlide,
    /// The last finite model, vowel and formant shift, which a NaN or
    /// infinite knob holds.
    held: [f32; 3],
}

impl Channel {
    fn new(rates: &Rates) -> Self {
        Self {
            naive: NaiveSynth::new(rates),
            ..Self::default()
        }
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            naive: NaiveSynth::default(),
            sam: SamSynth::default(),
            lpc: LpcVowels::default(),
            limiter: Limiter::default(),
            random: Random::new(0),
            seeded: false,
            trigger: SchmittTrigger::default(),
            model: SwapGlide::default(),
            vowel: SwapGlide::default(),
            formant_shift: SwapGlide::default(),
            held: [
                VowelParams::MODEL_DEFAULT,
                VowelParams::VOWEL_DEFAULT,
                VowelParams::FORMANT_SHIFT_DEFAULT,
            ],
        }
    }
}

impl Channel {
    /// One sample at `f0` cycles per sample: the limited voice and the
    /// excitation, before the output gains. A rising `trigger` starts a
    /// syllable; `None` is an unpatched trigger.
    #[inline(always)]
    fn step(&mut self, f0: f32, knobs: Knobs, trigger: Option<f32>, rates: &Rates) -> (f32, f32) {
        let trigger = match trigger {
            Some(volts) => self.trigger.process(volts),
            None => {
                // A trigger patched again starts from low.
                self.trigger = SchmittTrigger::default();
                false
            }
        };
        let sample_rate = rates.sample_rate;
        let raw = [
            self.model.follow(knobs.model.0, knobs.model.1, sample_rate),
            self.vowel.follow(knobs.vowel.0, knobs.vowel.1, sample_rate),
            self.formant_shift
                .follow(knobs.formant_shift.0, knobs.formant_shift.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let [model, vowel, formant_shift] = self.held.map(knob);
        let f0 = limit_frequency(f0, 0.5);

        // Plaits renders the formant-filter voice or the LPC voice beside
        // SAM, whichever side of it `model` is on; the other holds still, as
        // does a voice the blend gives no weight, such as SAM at 0 and 5 and
        // the formant filters at 2.5.
        let group = 2.0 * model;
        let lpc_side = group > 1.0;
        let blend = if lpc_side { 2.0 - group } else { group };
        let side = (blend < 1.0).then(|| {
            if lpc_side {
                let [excitation, out] = self.lpc.step(
                    trigger,
                    f0 * rates.lpc_pitch_scale,
                    vowel,
                    formant_shift,
                    &mut self.random,
                    rates,
                );
                (excitation, out)
            } else {
                self.naive.step(trigger, f0, vowel, formant_shift, rates)
            }
        });
        let sam = (blend > 0.0).then(|| self.sam.step(trigger, f0, vowel, formant_shift, rates));
        let (excitation, out) = match (side, sam) {
            (Some((excitation, out)), Some((sam_excitation, sam_out))) => {
                let blend = crossfade(blend);
                (
                    excitation + (sam_excitation - excitation) * blend,
                    out + (sam_out - out) * blend,
                )
            }
            (Some(side), None) => side,
            (None, Some(sam)) => sam,
            (None, None) => unreachable!("a blend between 0 and 1 weights a voice"),
        };
        (
            self.limiter.process(out * PRE_GAIN, &rates.limiter),
            excitation,
        )
    }
}

impl Vowel {
    fn init(&mut self, sample_rate: f32) {
        self.state = Rates::new(sample_rate);
        for channel in self.channel_state.iter_mut() {
            *channel = Channel::new(&self.state);
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, VowelParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let trigger = p.trigger.as_ref().map(|t| t.get_value(ch));
            let knobs = Knobs {
                model: (
                    p.model.value_or(ch, VowelParams::MODEL_DEFAULT),
                    p.model.glide_mark(ch),
                ),
                vowel: (
                    p.vowel.value_or(ch, VowelParams::VOWEL_DEFAULT),
                    p.vowel.glide_mark(ch),
                ),
                formant_shift: (
                    p.formant_shift
                        .value_or(ch, VowelParams::FORMANT_SHIFT_DEFAULT),
                    p.formant_shift.glide_mark(ch),
                ),
            };
            let (out, excitation) = channel.step(f0, knobs, trigger, &self.state);
            self.outputs
                .sample
                .set(ch, (OUTPUT_GAIN * out).clamp(-5.0, 5.0));
            self.outputs
                .excitation
                .set(ch, (EXCITATION_GAIN * excitation).clamp(-5.0, 5.0));
        }
    }
}

impl crate::types::PatchUpdateHandler for Vowel {
    /// Seeds the noise generators of channels that have none yet. Runs here,
    /// not in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same noise.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, channel) in self.channel_state.iter_mut().enumerate() {
            if !channel.seeded {
                let unit = (hash_bipolar(base, ch as u64) + 1.0) * 0.5;
                channel.random = Random::new((unit * f64::from(u32::MAX)) as u32);
                channel.seeded = true;
            }
        }
    }
}

message_handlers!(impl Vowel {});

#[cfg(test)]
mod tests;
