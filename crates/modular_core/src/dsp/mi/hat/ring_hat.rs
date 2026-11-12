//! `$mi.ringHat`: Plaits' ring-modulated hi-hat.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{oscillators::FmMode, utils::rng::seed_base},
    poly::{PolyOutput, PolySignal, PolySignalExt},
};

use super::channel::{ACCENT_DEFAULT, ACCENT_TRIM, Channel, HatState, KNOB_DEFAULT, Knobs, seed};
use super::hat_drum::RingHat;
use crate::dsp::mi::accent::plaits_volts;
use crate::dsp::mi::drum::{OUTPUT_GAIN, pitch};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiRingHatParams {
    #[input(
        "the metallic tone's pitch: its partials climb as it rises, levelling off above about 2 kHz, and the clocked noise follows",
        type = pitch,
    )]
    freq: PolySignal,
    #[input(
        "strikes the hat. Unpatched, the hat hisses steadily, as loud as `accent` and `decay` together allow",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "the hat's colour: the band its metal sounds in, from a dull 150 Hz (0) to a bright 9.6 kHz (5); a high-pass at the same frequency thins what lies below",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "how long the hat rings, from a closed tick (0) to an open wash (5); its tail is choked faster once it falls to about a third of a full strike",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "blends noise into the metal, from none (0) to all noise (5), slowly at first; the noise is clocked at 32 down to 16 times the pitch as it rises",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    noisiness: Option<PolySignal>,
    #[input(
        "how hard each hit lands, read at each strike: up to twice as loud at 5 V, and longer before its tail is choked; unpatched trigger, raises the hiss's level",
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

const _: () = assert!(
    MiRingHatParams::TONE_DEFAULT == KNOB_DEFAULT
        && MiRingHatParams::DECAY_DEFAULT == KNOB_DEFAULT
        && MiRingHatParams::NOISINESS_DEFAULT == KNOB_DEFAULT
        && MiRingHatParams::ACCENT_DEFAULT == ACCENT_DEFAULT
);

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MiRingHatOutputs {
    #[output("output", "the hat; an unaccented hit peaks within ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const RING_HAT_DOCS: ModuleDocs = ModuleDocs {
    summary: "Ring-modulated hi-hat",
    description: "Mutable Instruments Plaits' second hi-hat: three \
                  ring-modulated square and saw pairs make a harsher metallic \
                  noise, band-passed, then shaped by an envelope that chokes \
                  its tail and a high-pass. Leave `trigger` unpatched and it \
                  hisses steadily.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[
        DocExample {
            title: "Sixteenths",
            code: r#"
                const trigger = $p('c*16').c
                const hat = $mi.ringHat('a3', { trigger })
                hat.out()
            "#,
        },
        DocExample {
            title: "Choked open hat on the off-beat",
            code: r#"
                const trigger = $p('~ c').c
                const hat = $mi.ringHat('a3', {
                  trigger,
                  tone: 3.5,
                  decay: 4.5,
                  noisiness: 1,
                })
                hat.out()
            "#,
        },
    ],
};

#[module(name = "$mi.ringHat", docs = RING_HAT_DOCS, args(freq), has_init, patch_update)]
pub struct MiRingHat {
    outputs: MiRingHatOutputs,
    state: HatState,
    channel_state: Box<[Channel<RingHat>]>,
    params: MiRingHatParams,
}

impl MiRingHat {
    fn init(&mut self, sample_rate: f32) {
        self.state = HatState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let state = &self.state;
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = pitch(
                p.freq.get_value(ch),
                p.fm.value_or(ch, MiRingHatParams::FM_DEFAULT),
                p.fm_mode,
                state.inv_sample_rate,
            );
            let trigger = p.trigger.as_ref().map(|t| t.get_value(ch));
            let knob = |signal: &Option<PolySignal>, default| {
                (signal.value_or(ch, default), signal.glide_mark(ch))
            };
            let accent = knob(&p.accent, MiRingHatParams::ACCENT_DEFAULT);
            let mark = accent.1;
            let accent = channel
                .accent_state
                .read(accent, trigger, state.sample_rate);
            let knobs = Knobs {
                tone: knob(&p.tone, MiRingHatParams::TONE_DEFAULT),
                decay: knob(&p.decay, MiRingHatParams::DECAY_DEFAULT),
                noisiness: knob(&p.noisiness, MiRingHatParams::NOISINESS_DEFAULT),
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

impl crate::types::PatchUpdateHandler for MiRingHat {
    fn on_patch_update(&mut self) {
        seed(seed_base(self), &mut self.channel_state);
    }
}

message_handlers!(impl MiRingHat {});
