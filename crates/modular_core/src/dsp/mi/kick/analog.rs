//! `$mi.kick`: Plaits' analog bass drum through its overdrive.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{oscillators::FmMode, utils::SchmittTrigger},
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::GlideMark,
};

use super::analog_drum::{AnalogBassDrum, AnalogKnobs, AnalogRates, Overdrive};
use crate::dsp::mi::accent::{Accent, plaits_volts};
use crate::dsp::mi::drum::{Knob, OUTPUT_GAIN, pitch, rises};

/// The output level at full accent, which with Plaits' own response to
/// accent makes a fully accented hit peak twice as high as an unaccented one.
const ACCENT_TRIM: f32 = 1.79;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiKickParams {
    #[input("the drum's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "strikes the drum. Unpatched, the drum drones: a sine at the pitch, bent by `selfFm`, as loud as `accent` and `decay` together allow",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "how bright the hit is: a muffled thump (0) to a bright body with a clicky attack (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "how long the drum rings, from a short thud (0) to a long boom (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "how far the pitch jumps up for the first few milliseconds of each hit, for a sharper attack",
        type = control,
        default = 5.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    attack_fm: Option<PolySignal>,
    #[input(
        "how far the pitch rises with the drum's level, so it bends down as the hit fades",
        type = control,
        default = 5.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    self_fm: Option<PolySignal>,
    #[input(
        "overdrive, from a gentle saturation (0) to a heavy one (5); it eases off on high notes and is gone by 3 kHz",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    drive: Option<PolySignal>,
    #[input(
        "how hard each hit lands, read at each strike: up to twice as loud at 5 V, with a harder attack; unpatched trigger, raises the drone's level",
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
struct MiKickOutputs {
    #[output("output", "the drum; an unaccented hit peaks within ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const KICK_DOCS: ModuleDocs = ModuleDocs {
    summary: "Analog bass drum",
    description: "Mutable Instruments Plaits' 808-style bass drum: a pulse rings \
                  a resonant filter whose pitch jumps on the attack and bends \
                  with its own level, then an overdrive. Leave `trigger` \
                  unpatched and it drones as a sine.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[
        DocExample {
            title: "Four on the floor",
            code: "$mi.kick('g1', { trigger: $p('c*4'), decay: 3 }).out()",
        },
        DocExample {
            title: "Driven, bending kick",
            code: r#"
                const hits = $p('c ~ ~ c ~ ~ c ~')
                $mi.kick('e1', { trigger: hits, decay: 4, drive: 4 }).out()
            "#,
        },
    ],
};

#[module(name = "$mi.kick", docs = KICK_DOCS, args(freq), has_init)]
pub struct MiKick {
    outputs: MiKickOutputs,
    state: KickState,
    channel_state: Box<[Channel]>,
    params: MiKickParams,
}

pub(super) struct KickState {
    sample_rate: f32,
    inv_sample_rate: f32,
    /// The rate over 48 kHz.
    to_48k: f32,
    rates: AnalogRates,
}

impl Default for KickState {
    fn default() -> Self {
        Self::new(48000.0)
    }
}

impl KickState {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            rates: AnalogRates::new(sample_rate),
        }
    }
}

/// The knob inputs for a sample, in volts, with their glide marks.
pub(super) struct Knobs {
    pub tone: (f32, GlideMark),
    pub decay: (f32, GlideMark),
    pub attack_fm: (f32, GlideMark),
    pub self_fm: (f32, GlideMark),
    pub drive: (f32, GlideMark),
    pub accent: (f32, GlideMark),
}

/// One channel: the drum, then the overdrive.
pub(super) struct Channel {
    drum: AnalogBassDrum,
    overdrive: Overdrive,
    trigger: SchmittTrigger,
    tone: Knob,
    decay: Knob,
    attack_fm: Knob,
    self_fm: Knob,
    drive: Knob,
    accent: Knob,
    accent_state: Accent,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            drum: AnalogBassDrum::default(),
            overdrive: Overdrive::default(),
            trigger: SchmittTrigger::default(),
            tone: Knob::new(MiKickParams::TONE_DEFAULT),
            decay: Knob::new(MiKickParams::DECAY_DEFAULT),
            attack_fm: Knob::new(MiKickParams::ATTACK_FM_DEFAULT),
            self_fm: Knob::new(MiKickParams::SELF_FM_DEFAULT),
            drive: Knob::new(MiKickParams::DRIVE_DEFAULT),
            accent: Knob::new(plaits_volts(MiKickParams::ACCENT_DEFAULT)),
            accent_state: Accent::default(),
        }
    }
}

impl Channel {
    /// One sample at `f0` cycles per sample, before the output gain. A rising
    /// `trigger` strikes the drum; `None` is an unpatched trigger.
    #[inline(always)]
    pub(super) fn step(
        &mut self,
        f0: f32,
        knobs: Knobs,
        trigger: Option<f32>,
        state: &KickState,
    ) -> f32 {
        let triggered = rises(&mut self.trigger, trigger);
        let sample_rate = state.sample_rate;
        let drum_knobs = AnalogKnobs {
            accent: self.accent.read(knobs.accent, sample_rate),
            tone: self.tone.read(knobs.tone, sample_rate),
            decay: self.decay.read(knobs.decay, sample_rate),
            attack_fm: self.attack_fm.read(knobs.attack_fm, sample_rate),
            self_fm: self.self_fm.read(knobs.self_fm, sample_rate),
        };
        let drive = self.drive.read(knobs.drive, sample_rate);
        let out = self
            .drum
            .render(trigger.is_none(), triggered, f0, &drum_knobs, &state.rates);
        // The drive fades out toward 3 kHz.
        let drive = drive * (1.0 - 16.0 * (f0 * state.to_48k)).max(0.0);
        self.overdrive.process(0.5 + 0.5 * drive, out)
    }
}

impl MiKick {
    fn init(&mut self, sample_rate: f32) {
        self.state = KickState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let state = &self.state;
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = pitch(
                p.freq.get_value(ch),
                p.fm.value_or(ch, MiKickParams::FM_DEFAULT),
                p.fm_mode,
                state.inv_sample_rate,
            );
            let trigger = p.trigger.as_ref().map(|t| t.get_value(ch));
            let knob = |signal: &Option<PolySignal>, default| {
                (signal.value_or(ch, default), signal.glide_mark(ch))
            };
            let accent = knob(&p.accent, MiKickParams::ACCENT_DEFAULT);
            let mark = accent.1;
            let accent = channel
                .accent_state
                .read(accent, trigger, state.sample_rate);
            let knobs = Knobs {
                tone: knob(&p.tone, MiKickParams::TONE_DEFAULT),
                decay: knob(&p.decay, MiKickParams::DECAY_DEFAULT),
                attack_fm: knob(&p.attack_fm, MiKickParams::ATTACK_FM_DEFAULT),
                self_fm: knob(&p.self_fm, MiKickParams::SELF_FM_DEFAULT),
                drive: knob(&p.drive, MiKickParams::DRIVE_DEFAULT),
                accent: (plaits_volts(accent), mark),
            };
            let out = channel.step(f0, knobs, trigger, state);
            let out = channel.accent_state.apply(
                (OUTPUT_GAIN * out).clamp(-5.0, 5.0),
                ACCENT_TRIM,
                state.sample_rate,
            );
            self.outputs.sample.set(ch, out);
        }
    }
}

message_handlers!(impl MiKick {});
