//! `$mi.hiHat`: Plaits' 808-style hi-hat.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{oscillators::FmMode, utils::rng::seed_base},
    poly::{PolyOutput, PolySignal, PolySignalExt},
};

use super::channel::{ACCENT_DEFAULT, Channel, HatState, KNOB_DEFAULT, Knobs, seed};
use super::hat_drum::SquareHat;
use crate::dsp::mi::drum::{OUTPUT_GAIN, pitch};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiHiHatParams {
    #[input(
        "the metallic tone's pitch: six square oscillators from twice it to five times it, with the clocked noise following",
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
        "the hat's colour: the band its metal rings in, from a dull 150 Hz (0) to a bright 9.6 kHz (5), ringing more as it rises; a high-pass at the same frequency thins what lies below",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "how long the hat rings, from a closed tick (0) to an open wash (5)",
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
        "how hard each hit is: louder; unpatched trigger, scales the hiss's level",
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

const _: () = assert!(
    MiHiHatParams::TONE_DEFAULT == KNOB_DEFAULT
        && MiHiHatParams::DECAY_DEFAULT == KNOB_DEFAULT
        && MiHiHatParams::NOISINESS_DEFAULT == KNOB_DEFAULT
        && MiHiHatParams::ACCENT_DEFAULT == ACCENT_DEFAULT
);

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MiHiHatOutputs {
    #[output("output", "the hat", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

const HI_HAT_DOCS: ModuleDocs = ModuleDocs {
    summary: "808-style hi-hat",
    description: "Mutable Instruments Plaits' 808-style hi-hat: six square \
                  oscillators make a metallic noise, band-passed, then shaped \
                  by a swinging VCA and a high-pass. Leave `trigger` \
                  unpatched and it hisses steadily.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[
        DocExample {
            title: "Eighths",
            code: r#"
                const trigger = $p('c*8').c
                const hat = $mi.hiHat('a3', { trigger })
                hat.out()
            "#,
        },
        DocExample {
            title: "Open, noisy hat on the off-beat",
            code: r#"
                const trigger = $p('~ c').c
                const hat = $mi.hiHat('a3', {
                  trigger,
                  tone: 4,
                  decay: 4,
                  noisiness: 3.5,
                })
                hat.out()
            "#,
        },
    ],
};

#[module(name = "$mi.hiHat", docs = HI_HAT_DOCS, args(freq), has_init, patch_update)]
pub struct MiHiHat {
    outputs: MiHiHatOutputs,
    state: HatState,
    channel_state: Box<[Channel<SquareHat>]>,
    params: MiHiHatParams,
}

impl MiHiHat {
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
                p.fm.value_or(ch, MiHiHatParams::FM_DEFAULT),
                p.fm_mode,
                state.inv_sample_rate,
            );
            let trigger = p.trigger.as_ref().map(|t| t.get_value(ch));
            let knob = |signal: &Option<PolySignal>, default| {
                (signal.value_or(ch, default), signal.glide_mark(ch))
            };
            let knobs = Knobs {
                tone: knob(&p.tone, MiHiHatParams::TONE_DEFAULT),
                decay: knob(&p.decay, MiHiHatParams::DECAY_DEFAULT),
                noisiness: knob(&p.noisiness, MiHiHatParams::NOISINESS_DEFAULT),
                accent: knob(&p.accent, MiHiHatParams::ACCENT_DEFAULT),
            };
            let out = channel.step(f0, knobs, trigger, state);
            self.outputs
                .sample
                .set(ch, (OUTPUT_GAIN * out).clamp(-5.0, 5.0));
        }
    }
}

impl crate::types::PatchUpdateHandler for MiHiHat {
    fn on_patch_update(&mut self) {
        seed(seed_base(self), &mut self.channel_state);
    }
}

message_handlers!(impl MiHiHat {});
