//! `$mi.synthKick`: Plaits' synthetic bass drum.

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

use super::synthetic_drum::{SyntheticBassDrum, SyntheticKnobs, SyntheticRates};
use super::{Knob, OUTPUT_GAIN, pitch, rises};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiSynthKickParams {
    #[input("the drum's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "strikes the drum. Unpatched, the drum drones: its body at the pitch, as loud as `accent` and `decay` together allow",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "how bright the hit is and how loud its click and noise: a soft, round thump (0) to a bright, clicky one (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "how long the body rings, from a short blip (0) to a long boom (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "grit: a clean sine body (0) to a jittery, pointed one (5); it eases off on high notes and is gone by 6 kHz",
        type = control,
        default = 1.6875,
        default_doc = "2 less 1.25 × (decay ÷ 5)², so a longer decay is cleaner",
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    dirt: Option<PolySignal>,
    #[input(
        "how far the pitch sweeps down onto the note at the start of each hit: none (0) to from 4.5 times the pitch (5)",
        type = control,
        default = 5.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    fm_envelope: Option<PolySignal>,
    #[input(
        "how long the pitch sweep takes, from 8 ms (0) to 40 ms (5)",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    fm_decay: Option<PolySignal>,
    #[input(
        "how hard each hit is: louder, with a harder attack; unpatched trigger, scales the drone's level",
        type = control,
        default = 4.0,
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
struct MiSynthKickOutputs {
    #[output("output", "the drum", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

const SYNTH_KICK_DOCS: ModuleDocs = ModuleDocs {
    summary: "Synthetic bass drum",
    description: "Mutable Instruments Plaits' synthetic bass drum, a little like \
                  a 909: a sine swept down in pitch, with a click and a burst \
                  of noise on the attack. Leave `trigger` unpatched and its \
                  body drones.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[
        DocExample {
            title: "Four on the floor",
            code: "$mi.synthKick('a1', { trigger: $p('c*4').c }).out()",
        },
        DocExample {
            title: "Long, gritty sweep",
            code: r#"
                const trigger = $p('c ~ c ~ ~ c ~ ~').c
                const kick = $mi.synthKick('e1', {
                  trigger,
                  decay: 4,
                  dirt: 4,
                  fmDecay: 3,
                })
                kick.out()
            "#,
        },
    ],
};

#[module(name = "$mi.synthKick", docs = SYNTH_KICK_DOCS, args(freq), has_init, patch_update)]
pub struct MiSynthKick {
    outputs: MiSynthKickOutputs,
    state: SynthKickState,
    channel_state: Box<[Channel]>,
    params: MiSynthKickParams,
}

pub(super) struct SynthKickState {
    sample_rate: f32,
    inv_sample_rate: f32,
    pub rates: SyntheticRates,
}

impl Default for SynthKickState {
    fn default() -> Self {
        Self::new(48000.0)
    }
}

impl SynthKickState {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            rates: SyntheticRates::new(sample_rate),
        }
    }
}

/// The knob inputs for a sample, in volts, with their glide marks; `dirt`'s
/// value is `None` while unpatched.
pub(super) struct Knobs {
    pub tone: (f32, GlideMark),
    pub decay: (f32, GlideMark),
    pub dirt: (Option<f32>, GlideMark),
    pub fm_envelope: (f32, GlideMark),
    pub fm_decay: (f32, GlideMark),
    pub accent: (f32, GlideMark),
}

pub(super) struct Channel {
    drum: SyntheticBassDrum,
    /// The channel's noise, for the phase jitter and the attack.
    random: Random,
    trigger: SchmittTrigger,
    tone: Knob,
    decay: Knob,
    dirt: Knob,
    fm_envelope: Knob,
    fm_decay: Knob,
    accent: Knob,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// channel, so its noise stays continuous across patch updates.
    seeded: bool,
}

impl Default for Channel {
    fn default() -> Self {
        Self::new(0, &SyntheticRates::new(48000.0))
    }
}

impl Channel {
    pub(super) fn new(seed: u32, rates: &SyntheticRates) -> Self {
        Self {
            drum: SyntheticBassDrum::new(rates),
            random: Random::new(seed),
            trigger: SchmittTrigger::default(),
            tone: Knob::new(MiSynthKickParams::TONE_DEFAULT),
            decay: Knob::new(MiSynthKickParams::DECAY_DEFAULT),
            dirt: Knob::new(MiSynthKickParams::DIRT_DEFAULT),
            fm_envelope: Knob::new(MiSynthKickParams::FM_ENVELOPE_DEFAULT),
            fm_decay: Knob::new(MiSynthKickParams::FM_DECAY_DEFAULT),
            accent: Knob::new(MiSynthKickParams::ACCENT_DEFAULT),
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
        state: &SynthKickState,
    ) -> f32 {
        let triggered = rises(&mut self.trigger, trigger);
        let sample_rate = state.sample_rate;
        let decay = self.decay.read(knobs.decay, sample_rate);
        // Unpatched, `dirt` follows the decay, and its glide tracks that so
        // patching it glides from what is sounding.
        let (dirt, mark) = knobs.dirt;
        let dirt = dirt.unwrap_or(5.0 * (0.4 - 0.25 * decay * decay));
        let dirtiness = self.dirt.read((dirt, mark), sample_rate);
        let drum_knobs = SyntheticKnobs {
            accent: self.accent.read(knobs.accent, sample_rate),
            tone: self.tone.read(knobs.tone, sample_rate),
            decay,
            dirtiness,
            fm_envelope: self.fm_envelope.read(knobs.fm_envelope, sample_rate),
            fm_decay: self.fm_decay.read(knobs.fm_decay, sample_rate),
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

impl MiSynthKick {
    fn init(&mut self, sample_rate: f32) {
        self.state = SynthKickState::new(sample_rate);
        for channel in self.channel_state.iter_mut() {
            *channel = Channel::new(0, &self.state.rates);
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let state = &self.state;
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = pitch(
                p.freq.get_value(ch),
                p.fm.value_or(ch, MiSynthKickParams::FM_DEFAULT),
                p.fm_mode,
                state.inv_sample_rate,
            );
            let trigger = p.trigger.as_ref().map(|t| t.get_value(ch));
            let knob = |signal: &Option<PolySignal>, default| {
                (signal.value_or(ch, default), signal.glide_mark(ch))
            };
            let knobs = Knobs {
                tone: knob(&p.tone, MiSynthKickParams::TONE_DEFAULT),
                decay: knob(&p.decay, MiSynthKickParams::DECAY_DEFAULT),
                dirt: (
                    p.dirt.as_ref().map(|dirt| dirt.get_value(ch)),
                    p.dirt.glide_mark(ch),
                ),
                fm_envelope: knob(&p.fm_envelope, MiSynthKickParams::FM_ENVELOPE_DEFAULT),
                fm_decay: knob(&p.fm_decay, MiSynthKickParams::FM_DECAY_DEFAULT),
                accent: knob(&p.accent, MiSynthKickParams::ACCENT_DEFAULT),
            };
            let out = channel.step(f0, knobs, trigger, state);
            self.outputs
                .sample
                .set(ch, (OUTPUT_GAIN * out).clamp(-5.0, 5.0));
        }
    }
}

impl crate::types::PatchUpdateHandler for MiSynthKick {
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

message_handlers!(impl MiSynthKick {});
