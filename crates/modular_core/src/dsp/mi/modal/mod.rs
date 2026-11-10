mod resonator;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            limiter::{Limiter, LimiterRates},
            phase::limit_frequency,
            random::Random,
            units::semitones_to_ratio,
        },
        oscillators::{FmMode, apply_fm},
        utils::{SchmittTrigger, rng::seed_base},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use resonator::{Resonator, svf_coefficients};

/// Plaits' output gains for this engine, at ±5 V full scale; the output also
/// passes Plaits' limiter.
const OUTPUT_GAIN: f32 = -5.0;
const EXCITER_GAIN: f32 = -0.8 * 5.0;

/// Lowest excitation cutoff, in cycles per sample at 48 kHz: far below any
/// pitch, and keeps a strike at zero pitch, whose height divides by the
/// cutoff, finite.
const MIN_CUTOFF: f32 = 1e-6;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ModalParams {
    #[input("the pitch the body rings at", type = pitch)]
    freq: PolySignal,
    #[input(
        "strikes the body; unpatched, it is excited continuously by a stream of tiny grains instead",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "spacing of the partials: squeezed together (0), harmonic around 1.25–1.5, then stretched apart into bells, plates and clangs (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    structure: Option<PolySignal>,
    #[input(
        "how bright the strike is and how long the upper partials last; unpatched trigger, how dense the grains are",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    brightness: Option<PolySignal>,
    #[input(
        "how long the body rings, from a dead thud (0) to a long sustain (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "how hard each strike is: louder, brighter and longer",
        type = control,
        default = 4.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input("a sound to ring the body with, alongside its own strikes", type = audio)]
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
struct ModalOutputs {
    #[output("output", "the ringing body", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
    #[output(
        "exciter",
        "the strike or grains that excite the body, to mix back in for more attack",
        type = cv,
        range = (-5.0, 5.0),
    )]
    exciter: PolyOutput,
}

const MODAL_DOCS: ModuleDocs = ModuleDocs {
    summary: "Modal resonator",
    description: "Mutable Instruments Plaits' modal resonator: 24 tuned modes \
                  struck by a filtered impulse, for bells, bars, plates, \
                  marimbas and gongs. Leave `trigger` unpatched and grains \
                  bow it into a continuous drone.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Marimba melody",
            code: r#"
                const seq = $cycle($p('c3 e3 g3 <a3 b3>'))
                $mi.modal(seq, { trigger: seq.trig, structure: 1.4 }).out()
            "#,
        },
        DocExample {
            title: "Bowed bell drone",
            code: "$mi.modal('c2', { structure: 4, decay: 4 }).out()",
        },
    ],
};

#[module(name = "$mi.modal", docs = MODAL_DOCS, args(freq), has_init)]
pub struct Modal {
    outputs: ModalOutputs,
    state: ModalState,
    channel_state: Box<[Voice]>,
    params: ModalParams,
}

#[derive(Default)]
struct ModalState {
    sample_rate: f32,
    inv_sample_rate: f32,
    /// Cycles per sample here to cycles per sample at Plaits' 48 kHz.
    to_48k: f32,
    /// Plaits' structure smoothing (0.01 per 12-sample block at 48 kHz) as a
    /// per-sample one-pole coefficient.
    structure_smoothing: f32,
    limiter: LimiterRates,
}

impl ModalState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            structure_smoothing: (1.0 - 0.99f64.powf(1.0 / 12.0 * 48000.0 / f64::from(sample_rate)))
                as f32,
            limiter: LimiterRates::new(sample_rate),
        }
    }
}

/// The knob inputs for a sample, in volts, with their glide marks.
struct Knobs {
    structure: (f32, GlideMark),
    brightness: (f32, GlideMark),
    decay: (f32, GlideMark),
    accent: (f32, GlideMark),
}

/// The low-pass that shapes the strike, cached on its cutoff and resonance.
#[derive(Clone, Copy)]
struct ExcitationFilter {
    state_1: f32,
    state_2: f32,
    key: [f32; 2],
    g: f32,
    r_plus_g: f32,
    h: f32,
}

impl Default for ExcitationFilter {
    fn default() -> Self {
        Self {
            state_1: 0.0,
            state_2: 0.0,
            key: [f32::NAN; 2],
            g: 0.0,
            r_plus_g: 0.0,
            h: 0.0,
        }
    }
}

impl ExcitationFilter {
    #[inline(always)]
    fn process(&mut self, cutoff: f32, q: f32, input: f32) -> f32 {
        let key = [cutoff, q];
        if key.map(f32::to_bits) != self.key.map(f32::to_bits) {
            (self.g, self.r_plus_g, self.h) = svf_coefficients(cutoff, q);
            self.key = key;
        }
        let hp = (input - self.r_plus_g * self.state_1 - self.state_2) * self.h;
        let bp = self.g * hp + self.state_1;
        self.state_1 = self.g * hp + bp;
        let lp = self.g * bp + self.state_2;
        self.state_2 = self.g * bp + lp;
        lp
    }
}

#[derive(Clone, Copy)]
struct Voice {
    excitation: ExcitationFilter,
    resonator: Resonator,
    limiter: Limiter,
    random: Random,
    trigger: SchmittTrigger,
    structure_lp: f32,
    structure: SwapGlide,
    brightness: SwapGlide,
    decay: SwapGlide,
    accent: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 4],
}

impl Voice {
    fn new(seed: u32) -> Self {
        Self {
            excitation: ExcitationFilter::default(),
            resonator: Resonator::default(),
            limiter: Limiter::default(),
            random: Random::new(seed),
            trigger: SchmittTrigger::default(),
            structure_lp: 0.0,
            structure: SwapGlide::default(),
            brightness: SwapGlide::default(),
            decay: SwapGlide::default(),
            accent: SwapGlide::default(),
            held: [
                ModalParams::STRUCTURE_DEFAULT,
                ModalParams::BRIGHTNESS_DEFAULT,
                ModalParams::DECAY_DEFAULT,
                ModalParams::ACCENT_DEFAULT,
            ],
        }
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::new(0)
    }
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Voice {
    /// One sample of the limited body and the exciter, before the output
    /// gains. `sustain` excites with grains rather than strikes; `strike`
    /// strikes this sample.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        f0: f32,
        knobs: Knobs,
        sustain: bool,
        strike: bool,
        input: f32,
        state: &ModalState,
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
        self.structure_lp += state.structure_smoothing * (knob(self.held[0]) - self.structure_lp);
        let mut brightness = knob(self.held[1]);
        let mut decay = knob(self.held[2]);
        let accent = knob(self.held[3]);
        let to_48k = state.to_48k;

        let density = brightness * brightness;
        brightness += 0.25 * accent * (1.0 - brightness);
        decay += 0.25 * accent * (1.0 - decay);
        let range = if sustain { 36.0 } else { 60.0 };
        let f = if sustain { 4.0 * f0 } else { 2.0 * f0 };
        let cutoff =
            (f * to_48k * semitones_to_ratio((brightness * (2.0 - brightness) - 0.5) * range))
                .clamp(MIN_CUTOFF, 0.499);
        let q = if sustain { 0.7 } else { 1.5 };

        // Impulses carry the same energy at any rate: a sample's height scales
        // with the rate.
        let excitation = if sustain {
            let dust_density = 0.00005 + 0.99995 * density * density;
            self.random.dust(dust_density / to_48k) * (4.0 - dust_density * 3.0) * accent * to_48k
        } else if strike {
            let attenuation = 1.0 - decay * 0.5;
            let amplitude = (0.12 + 0.08 * accent) * attenuation;
            amplitude * semitones_to_ratio(cutoff * cutoff * 24.0) / cutoff * to_48k
        } else {
            0.0
        };
        let exciter = self
            .excitation
            .process((cutoff / to_48k).min(0.499), q, excitation);
        let body = self.resonator.process(
            f0,
            self.structure_lp,
            brightness,
            decay,
            exciter + input,
            to_48k,
        );
        (self.limiter.process(body, &state.limiter), exciter)
    }
}

impl Modal {
    fn init(&mut self, sample_rate: f32) {
        self.state = ModalState::new(sample_rate);
        let seed = seed_base(self) as u32;
        for (ch, voice) in self.channel_state.iter_mut().enumerate() {
            *voice = Voice::new(seed ^ (ch as u32).wrapping_mul(0x9E37_79B9));
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let sustain = p.trigger.is_none();
        for (ch, voice) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, ModalParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let f0 = limit_frequency(f0.abs(), 0.5);
            let strike = p
                .trigger
                .as_ref()
                .is_some_and(|t| voice.trigger.process(t.get_value(ch)));
            let knobs = Knobs {
                structure: (
                    p.structure.value_or(ch, ModalParams::STRUCTURE_DEFAULT),
                    p.structure.glide_mark(ch),
                ),
                brightness: (
                    p.brightness.value_or(ch, ModalParams::BRIGHTNESS_DEFAULT),
                    p.brightness.glide_mark(ch),
                ),
                decay: (
                    p.decay.value_or(ch, ModalParams::DECAY_DEFAULT),
                    p.decay.glide_mark(ch),
                ),
                accent: (
                    p.accent.value_or(ch, ModalParams::ACCENT_DEFAULT),
                    p.accent.glide_mark(ch),
                ),
            };
            let input = p.input.value_or(ch, 0.0) * 0.2;
            let input = if input.is_finite() { input } else { 0.0 };
            let (body, exciter) = voice.step(f0, knobs, sustain, strike, input, &self.state);
            self.outputs.sample.set(ch, OUTPUT_GAIN * body);
            self.outputs.exciter.set(ch, EXCITER_GAIN * exciter);
        }
    }
}

message_handlers!(impl Modal {});

#[cfg(test)]
mod tests;
