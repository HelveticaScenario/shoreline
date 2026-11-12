//! `$mi.snare`: Plaits' analog snare drum.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::random::Random,
        oscillators::FmMode,
        utils::{
            SchmittTrigger,
            rng::{hash_bipolar, seed_base},
        },
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::GlideMark,
};

use super::analog_drum::{AnalogKnobs, AnalogRates, AnalogSnareDrum};
use crate::dsp::mi::accent::{AccentLevel, plaits_volts};
use crate::dsp::mi::drum::{Knob, OUTPUT_GAIN, pitch, rises};

/// The output level at full accent, which with Plaits' own response to
/// accent makes a fully accented hit peak twice as high as an unaccented one.
const ACCENT_TRIM: f32 = 1.94;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiSnareParams {
    #[input("the drum's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "strikes the drum. Unpatched, the drum drones: its shell as steady sines and its snares as a steady hiss, as loud as `accent` and `decay` together allow",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "the shell's colour: an 808's two tones, the higher one coming up (to 3.3), then three higher overtones joining in (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "how long the shell and snares ring, from a tight crack (0) to a long ring (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "the snares' rattle against the shell: all shell (0) to all snares (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    snappy: Option<PolySignal>,
    #[input(
        "how hard each hit lands, read at each strike: up to twice as loud at 5 V, the shell struck harder; unpatched trigger, raises the drone's level",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
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
struct MiSnareOutputs {
    #[output("output", "the drum; an unaccented hit peaks within ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const SNARE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Analog snare drum",
    description: "Mutable Instruments Plaits' 808-style snare: a pulse rings \
                  the drum's shell modes, mixed with band-passed noise for the \
                  snares. Leave `trigger` unpatched and it drones.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[
        DocExample {
            title: "Backbeat",
            code: "$mi.snare('d3', { trigger: $p('~ c') }).out()",
        },
        DocExample {
            title: "Ringing, snappy roll",
            code: r#"
                const trigger = $p('~ c ~ [c c c c]')
                const snare = $mi.snare('a2', {
                  trigger,
                  tone: 4,
                  decay: 4,
                  snappy: 4,
                })
                snare.out()
            "#,
        },
    ],
};

#[module(name = "$mi.snare", docs = SNARE_DOCS, args(freq), has_init, patch_update)]
pub struct MiSnare {
    outputs: MiSnareOutputs,
    state: SnareState,
    channel_state: Box<[Channel]>,
    params: MiSnareParams,
}

pub(super) struct SnareState {
    sample_rate: f32,
    inv_sample_rate: f32,
    rates: AnalogRates,
}

impl Default for SnareState {
    fn default() -> Self {
        Self::new(48000.0)
    }
}

impl SnareState {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            rates: AnalogRates::new(sample_rate),
        }
    }
}

/// The knob inputs for a sample, in volts, with their glide marks.
pub(super) struct Knobs {
    pub tone: (f32, GlideMark),
    pub decay: (f32, GlideMark),
    pub snappy: (f32, GlideMark),
    pub accent: (f32, GlideMark),
}

pub(super) struct Channel {
    drum: AnalogSnareDrum,
    /// The channel's noise, for the snares.
    random: Random,
    trigger: SchmittTrigger,
    tone: Knob,
    decay: Knob,
    snappy: Knob,
    accent: Knob,
    accent_level: AccentLevel,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// channel, so its noise stays continuous across patch updates.
    seeded: bool,
}

impl Default for Channel {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Channel {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            drum: AnalogSnareDrum::default(),
            random: Random::new(seed),
            trigger: SchmittTrigger::default(),
            tone: Knob::new(MiSnareParams::TONE_DEFAULT),
            decay: Knob::new(MiSnareParams::DECAY_DEFAULT),
            snappy: Knob::new(MiSnareParams::SNAPPY_DEFAULT),
            accent: Knob::new(plaits_volts(MiSnareParams::ACCENT_DEFAULT)),
            accent_level: AccentLevel::default(),
            seeded: false,
        }
    }

    /// One sample at `f0` cycles per sample, before the output gain. A rising
    /// `trigger` strikes the drum; `None` is an unpatched trigger.
    #[inline(always)]
    pub(super) fn step(
        &mut self,
        f0: f32,
        knobs: Knobs,
        trigger: Option<f32>,
        state: &SnareState,
    ) -> f32 {
        let triggered = rises(&mut self.trigger, trigger);
        let sample_rate = state.sample_rate;
        let drum_knobs = AnalogKnobs {
            accent: self.accent.read(knobs.accent, sample_rate),
            tone: self.tone.read(knobs.tone, sample_rate),
            decay: self.decay.read(knobs.decay, sample_rate),
            snappy: self.snappy.read(knobs.snappy, sample_rate),
        };
        self.drum.render(
            trigger.is_none(),
            triggered,
            f0,
            &drum_knobs,
            &mut self.random,
            &state.rates,
        )
    }
}

impl MiSnare {
    fn init(&mut self, sample_rate: f32) {
        self.state = SnareState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let state = &self.state;
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = pitch(
                p.freq.get_value(ch),
                p.fm.value_or(ch, MiSnareParams::FM_DEFAULT),
                p.fm_mode,
                state.inv_sample_rate,
            );
            let trigger = p.trigger.as_ref().map(|t| t.get_value(ch));
            let knob = |signal: &Option<PolySignal>, default| {
                (signal.value_or(ch, default), signal.glide_mark(ch))
            };
            let accent = knob(&p.accent, MiSnareParams::ACCENT_DEFAULT);
            let knobs = Knobs {
                tone: knob(&p.tone, MiSnareParams::TONE_DEFAULT),
                decay: knob(&p.decay, MiSnareParams::DECAY_DEFAULT),
                snappy: knob(&p.snappy, MiSnareParams::SNAPPY_DEFAULT),
                accent: (plaits_volts(accent.0), accent.1),
            };
            let out = channel.step(f0, knobs, trigger, state);
            let level = channel
                .accent_level
                .next(accent, trigger, ACCENT_TRIM, state.sample_rate);
            self.outputs
                .sample
                .set(ch, (OUTPUT_GAIN * out).clamp(-5.0, 5.0) * level);
        }
    }
}

impl crate::types::PatchUpdateHandler for MiSnare {
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

message_handlers!(impl MiSnare {});
