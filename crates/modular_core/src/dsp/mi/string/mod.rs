mod model;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            filter::{Svf, tan_dirty},
            limiter::{Limiter, LimiterRates},
            phase::limit_frequency,
            random::Random,
            units::semitones_to_ratio,
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

use model::{StringModel, StringRates, g_at_rate};

/// Plaits' output gains for this engine, at ±5 V full scale; the output also
/// passes Plaits' limiter, and both clip at ±5 V as Plaits does.
const OUTPUT_GAIN: f32 = -5.0;
const EXCITER_GAIN: f32 = -0.8 * 5.0;

/// Strings per channel. A strike moves to the next, so earlier notes ring on.
const STRINGS: usize = 3;

/// Lowest excitation cutoff, in cycles per sample at 48 kHz.
const MIN_CUTOFF: f32 = 1e-6;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiStringParams {
    #[input("the string's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "plucks a string; earlier notes ring on on their own strings. Unpatched, a stream of tiny grains bows the string continuously instead",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "inharmonicity: a curved bridge that buzzes and sitar-twangs (0), a clean string around 1.25, then dispersion stretching the partials toward piano wire and metal (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    structure: Option<PolySignal>,
    #[input(
        "how bright the pluck is and how long the high partials last; unpatched trigger, how dense the grains are",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    brightness: Option<PolySignal>,
    #[input(
        "how long the string rings, from a muted thud (0) to sustaining forever (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "how hard each pluck is: louder, brighter and longer; unpatched trigger, how loud the grains are, silent at 0",
        type = control,
        default = 4.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input("a sound to resonate the current string with, alongside its plucks", type = audio)]
    #[deserr(default)]
    input: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); a negative frequency rings at its magnitude",
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
struct MiStringOutputs {
    #[output("strings", "the ringing strings", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
    #[output(
        "exciter",
        "the plucks or grains that excite the strings, to mix back in for more attack",
        type = cv,
        range = (-5.0, 5.0),
    )]
    exciter: PolyOutput,
}

const STRING_DOCS: ModuleDocs = ModuleDocs {
    summary: "Plucked and bowed string",
    description: "Mutable Instruments Plaits' inharmonic string: a \
                  Karplus-Strong string plucked by a burst of filtered noise, \
                  for guitars, sitars, harps and piano wire. Each pluck takes \
                  the next of three strings so notes overlap; leave `trigger` \
                  unpatched and grains bow it into a drone.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Plucked arpeggio",
            code: r#"
                const seq = $p('c3 g3 e4 <b3 a3>').c
                $mi.string(seq, { trigger: seq }).out()
            "#,
        },
        DocExample {
            title: "Sitar drone",
            code: "$mi.string('d2', { structure: 0.3, decay: 4.5 }).out()",
        },
    ],
};

#[module(name = "$mi.string", docs = STRING_DOCS, args(freq), has_init, patch_update)]
pub struct MiString {
    outputs: MiStringOutputs,
    state: MiStringState,
    channel_state: Box<[Voice]>,
    params: MiStringParams,
}

#[derive(Default)]
struct MiStringState {
    sample_rate: f32,
    inv_sample_rate: f32,
    rates: StringRates,
    limiter: LimiterRates,
}

/// The knob inputs for a sample, in volts, with their glide marks.
struct Knobs {
    structure: (f32, GlideMark),
    brightness: (f32, GlideMark),
    decay: (f32, GlideMark),
    accent: (f32, GlideMark),
}

/// One string with its excitation: a noise burst a period long on a pluck,
/// or grains when bowed, through a low-pass.
#[derive(Default)]
struct StringVoice {
    excitation: Svf,
    model: StringModel,
    remaining_noise_samples: usize,
}

/// The controls every string of a channel shares for a sample.
struct Shared {
    accent: f32,
    structure: f32,
    brightness: f32,
    damping: f32,
}

impl StringVoice {
    fn new(rates: &StringRates) -> Self {
        Self {
            model: StringModel::new(rates),
            ..Self::default()
        }
    }

    /// One sample of this string and its excitation. `sustain` bows it with
    /// grains; `pluck` plucks it this sample.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        sustain: bool,
        pluck: bool,
        f0: f32,
        shared: &Shared,
        input: f32,
        random: &mut Random,
        rates: &StringRates,
    ) -> (f32, f32) {
        let accent = shared.accent;
        let density = shared.brightness * shared.brightness;
        let brightness = shared.brightness + 0.25 * accent * (1.0 - shared.brightness);
        let damping = shared.damping + 0.25 * accent * (1.0 - shared.damping);
        let to_48k = rates.to_48k;

        if pluck || sustain {
            let range = 72.0;
            let f = 4.0 * f0;
            let cutoff =
                (f * to_48k * semitones_to_ratio((brightness * (2.0 - brightness) - 0.5) * range))
                    .clamp(MIN_CUTOFF, 0.499);
            let q = if sustain { 1.0 } else { 0.5 };
            self.remaining_noise_samples = (1.0 / f0) as usize;
            self.excitation
                .set_g_q(g_at_rate(tan_dirty(cutoff), to_48k, tan_dirty(0.499)), q);
        }

        // Grains carry the same energy at any rate: a grain's height scales
        // with the rate.
        let excitation = if sustain {
            let dust_density = 0.00005 + 0.99995 * density * density;
            random.dust(dust_density / to_48k) * (8.0 - dust_density * 6.0) * accent * to_48k
        } else if self.remaining_noise_samples > 0 {
            self.remaining_noise_samples -= 1;
            (2.0 * random.float() - 1.0) * rates.noise_gain
        } else {
            0.0
        };
        let exciter = self.excitation.low_pass(excitation);

        let structure = shared.structure;
        let non_linearity = if structure < 0.24 {
            (structure - 0.24) * 4.166
        } else if structure > 0.26 {
            (structure - 0.26) * 1.35135
        } else {
            0.0
        };
        let string = self.model.process(
            f0,
            non_linearity,
            brightness,
            damping,
            exciter + input,
            random,
            rates,
        );
        (string, exciter)
    }
}

struct Voice {
    strings: [StringVoice; STRINGS],
    /// Each string's pitch, in cycles per sample; a string keeps its pitch
    /// once a pluck moves on from it.
    f0: [f32; STRINGS],
    active: usize,
    /// One generator for the channel, drawn by its strings in turn, as Plaits
    /// draws its global one.
    random: Random,
    limiter: Limiter,
    trigger: SchmittTrigger,
    structure: SwapGlide,
    brightness: SwapGlide,
    decay: SwapGlide,
    accent: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 4],
    /// Whether `random` has its per-instance seed. Transfers with the voice,
    /// so its noise stays continuous across patch updates.
    seeded: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            strings: Default::default(),
            f0: [0.01; STRINGS],
            active: STRINGS - 1,
            random: Random::new(0),
            limiter: Limiter::default(),
            trigger: SchmittTrigger::default(),
            structure: SwapGlide::default(),
            brightness: SwapGlide::default(),
            decay: SwapGlide::default(),
            accent: SwapGlide::default(),
            held: [
                MiStringParams::STRUCTURE_DEFAULT,
                MiStringParams::BRIGHTNESS_DEFAULT,
                MiStringParams::DECAY_DEFAULT,
                MiStringParams::ACCENT_DEFAULT,
            ],
            seeded: false,
        }
    }
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Voice {
    /// A voice with strings sized for `rates`, seeded with `seed`. Allocates.
    fn new(seed: u32, rates: &StringRates) -> Self {
        Self {
            strings: std::array::from_fn(|_| StringVoice::new(rates)),
            random: Random::new(seed),
            ..Self::default()
        }
    }

    /// One sample of the limited strings and their excitation, before the
    /// output gains. `sustain` bows with grains rather than plucks; `pluck`
    /// plucks the next string this sample.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        f0: f32,
        knobs: Knobs,
        sustain: bool,
        pluck: bool,
        input: f32,
        state: &MiStringState,
    ) -> (f32, f32) {
        let sample_rate = state.sample_rate;
        let raw = [
            self.structure
                .follow(knobs.structure.0, knobs.structure.1, sample_rate),
            self.brightness
                .follow(knobs.brightness.0, knobs.brightness.1, sample_rate),
            self.decay.follow(knobs.decay.0, knobs.decay.1, sample_rate),
            self.accent
                .follow(knobs.accent.0, knobs.accent.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let timbre = knob(self.held[1]);
        let shared = Shared {
            accent: knob(self.held[3]),
            structure: knob(self.held[0]),
            brightness: timbre * timbre,
            damping: knob(self.held[2]),
        };

        if pluck {
            self.active = (self.active + 1) % STRINGS;
        }
        self.f0[self.active] = f0;
        let mut out = 0.0;
        let mut aux = 0.0;
        for (i, string) in self.strings.iter_mut().enumerate() {
            let active = i == self.active;
            let (s, e) = string.render(
                sustain && active,
                pluck && active,
                self.f0[i],
                &shared,
                if active { input } else { 0.0 },
                &mut self.random,
                &state.rates,
            );
            out += s;
            aux += e;
        }
        (self.limiter.process(out, &state.limiter), aux)
    }
}

impl MiString {
    fn init(&mut self, sample_rate: f32) {
        self.state = MiStringState {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            rates: StringRates::new(sample_rate),
            limiter: LimiterRates::new(sample_rate),
        };
        for voice in self.channel_state.iter_mut() {
            *voice = Voice::new(0, &self.state.rates);
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let sustain = p.trigger.is_none();
        for (ch, voice) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, MiStringParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            // A 1 Hz floor keeps the string's length finite at a standstill.
            let f0 = limit_frequency(f0.abs(), 0.5).max(self.state.inv_sample_rate);
            let pluck = p
                .trigger
                .as_ref()
                .is_some_and(|t| voice.trigger.process(t.get_value(ch)));
            let knobs = Knobs {
                structure: (
                    p.structure.value_or(ch, MiStringParams::STRUCTURE_DEFAULT),
                    p.structure.glide_mark(ch),
                ),
                brightness: (
                    p.brightness
                        .value_or(ch, MiStringParams::BRIGHTNESS_DEFAULT),
                    p.brightness.glide_mark(ch),
                ),
                decay: (
                    p.decay.value_or(ch, MiStringParams::DECAY_DEFAULT),
                    p.decay.glide_mark(ch),
                ),
                accent: (
                    p.accent.value_or(ch, MiStringParams::ACCENT_DEFAULT),
                    p.accent.glide_mark(ch),
                ),
            };
            let input = p.input.value_or(ch, 0.0) * 0.2;
            let input = if input.is_finite() { input } else { 0.0 };
            let (strings, exciter) = voice.step(f0, knobs, sustain, pluck, input, &self.state);
            // Plaits clips after its slow limiter.
            self.outputs
                .sample
                .set(ch, (OUTPUT_GAIN * strings).clamp(-5.0, 5.0));
            self.outputs
                .exciter
                .set(ch, (EXCITER_GAIN * exciter).clamp(-5.0, 5.0));
        }
    }
}

impl crate::types::PatchUpdateHandler for MiString {
    /// Seeds the noise generators of voices that have none yet. Runs here,
    /// not in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same noise.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, voice) in self.channel_state.iter_mut().enumerate() {
            if !voice.seeded {
                let unit = (hash_bipolar(base, ch as u64) + 1.0) * 0.5;
                voice.random = Random::new((unit * f64::from(u32::MAX)) as u32);
                voice.seeded = true;
            }
        }
    }
}

message_handlers!(impl MiString {});

#[cfg(test)]
mod tests;
