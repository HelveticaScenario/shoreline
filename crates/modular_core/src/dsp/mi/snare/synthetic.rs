//! `$mi.synthSnare`: Plaits' synthetic snare drum.

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

use super::synthetic_drum::{SyntheticKnobs, SyntheticRates, SyntheticSnareDrum};
use crate::dsp::mi::accent::{AccentLevel, plaits_volts};
use crate::dsp::mi::drum::{Knob, OUTPUT_GAIN, pitch, rises};

/// The output level at full accent, which with Plaits' own response to
/// accent makes a fully accented hit peak twice as high as an unaccented one.
const ACCENT_TRIM: f32 = 1.76;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiSynthSnareParams {
    #[input("the drum's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "strikes the drum. Unpatched, the drum drones: its body and snares held steady, as loud as `accent` and `decay` together allow",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "how far the pitch sweeps down onto the note at the start of each hit, from 5 times the pitch (5); more also lengthens the body, and high settings roughen it on low and mid notes",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    attack_fm: Option<PolySignal>,
    #[input(
        "how long the body and snares ring, from a short tick (0) to a long tail (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "the snares' hiss against the body: all body (0) to all snares (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    snappy: Option<PolySignal>,
    #[input(
        "how hard each hit lands, read at each strike: up to twice as loud at 5 V; unpatched trigger, raises the drone's level",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); a negative frequency sounds at its magnitude",
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
struct MiSynthSnareOutputs {
    #[output("output", "the drum; an unaccented hit peaks within ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const SYNTH_SNARE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Synthetic snare drum",
    description: "Mutable Instruments Plaits' synthetic snare, a little like a \
                  909: two coupled oscillators swept down in pitch for the \
                  body, and filtered noise for the snares. Leave `trigger` \
                  unpatched and it drones.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[
        DocExample {
            title: "Backbeat",
            code: "$mi.synthSnare('d3', { trigger: $p('~ c') }).out()",
        },
        DocExample {
            title: "Tight, punchy hits",
            code: r#"
                const trigger = $p('~ c ~ c ~ c c ~')
                const snare = $mi.synthSnare('e3', {
                  trigger,
                  attackFm: 4,
                  decay: 1,
                  snappy: 3.5,
                })
                snare.out()
            "#,
        },
    ],
};

#[module(name = "$mi.synthSnare", docs = SYNTH_SNARE_DOCS, args(freq), has_init, patch_update)]
pub struct MiSynthSnare {
    outputs: MiSynthSnareOutputs,
    state: SynthSnareState,
    channel_state: Box<[Channel]>,
    params: MiSynthSnareParams,
}

pub(super) struct SynthSnareState {
    sample_rate: f32,
    inv_sample_rate: f32,
    rates: SyntheticRates,
}

impl Default for SynthSnareState {
    fn default() -> Self {
        Self::new(48000.0)
    }
}

impl SynthSnareState {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            rates: SyntheticRates::new(sample_rate),
        }
    }
}

/// The knob inputs for a sample, in volts, with their glide marks.
pub(super) struct Knobs {
    pub attack_fm: (f32, GlideMark),
    pub decay: (f32, GlideMark),
    pub snappy: (f32, GlideMark),
    pub accent: (f32, GlideMark),
}

pub(super) struct Channel {
    drum: SyntheticSnareDrum,
    /// The channel's noise, for the snares.
    random: Random,
    trigger: SchmittTrigger,
    attack_fm: Knob,
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
            drum: SyntheticSnareDrum::default(),
            random: Random::new(seed),
            trigger: SchmittTrigger::default(),
            attack_fm: Knob::new(MiSynthSnareParams::ATTACK_FM_DEFAULT),
            decay: Knob::new(MiSynthSnareParams::DECAY_DEFAULT),
            snappy: Knob::new(MiSynthSnareParams::SNAPPY_DEFAULT),
            accent: Knob::new(plaits_volts(MiSynthSnareParams::ACCENT_DEFAULT)),
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
        state: &SynthSnareState,
    ) -> f32 {
        let triggered = rises(&mut self.trigger, trigger);
        let sample_rate = state.sample_rate;
        let drum_knobs = SyntheticKnobs {
            accent: self.accent.read(knobs.accent, sample_rate),
            attack_fm: self.attack_fm.read(knobs.attack_fm, sample_rate),
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

impl MiSynthSnare {
    fn init(&mut self, sample_rate: f32) {
        self.state = SynthSnareState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let state = &self.state;
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = pitch(
                p.freq.get_value(ch),
                p.fm.value_or(ch, MiSynthSnareParams::FM_DEFAULT),
                p.fm_mode,
                state.inv_sample_rate,
            );
            let trigger = p.trigger.as_ref().map(|t| t.get_value(ch));
            let knob = |signal: &Option<PolySignal>, default| {
                (signal.value_or(ch, default), signal.glide_mark(ch))
            };
            let accent = knob(&p.accent, MiSynthSnareParams::ACCENT_DEFAULT);
            let knobs = Knobs {
                attack_fm: knob(&p.attack_fm, MiSynthSnareParams::ATTACK_FM_DEFAULT),
                decay: knob(&p.decay, MiSynthSnareParams::DECAY_DEFAULT),
                snappy: knob(&p.snappy, MiSynthSnareParams::SNAPPY_DEFAULT),
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

impl crate::types::PatchUpdateHandler for MiSynthSnare {
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

message_handlers!(impl MiSynthSnare {});
