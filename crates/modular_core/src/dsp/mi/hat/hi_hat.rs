//! `$mi.hiHat`: Plaits' two hi-hats on one trigger.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;
use crate::{
    dsp::{mi::core::blend::Blend, oscillators::FmMode, utils::rng::seed_base},
    poly::{PolyOutput, PolySignal, PolySignalExt},
};

use super::hat_drum::{RingHat, SquareHat};
use super::lane::{ACCENT_DEFAULT, HatState, KNOB_DEFAULT, Knobs, Lane, Strike, seed};
use crate::dsp::mi::drum::pitch;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiHiHatParams {
    #[input(
        "the metallic tone's pitch: `square`'s six oscillators sit from twice it to five times it, `ring`'s partials climb with it, levelling off above about 2 kHz, and the clocked noise follows",
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
        "the hat's colour: the band its metal sounds in, from a dull 150 Hz (0) to a bright 9.6 kHz (5), `square` ringing more as it rises; a high-pass at the same frequency thins what lies below",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "how long the hat rings, from a closed tick (0) to an open wash (5); `ring`'s tail is choked faster once it falls to about a third of a full strike",
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
        "how hard each hit lands, read at each strike: up to twice as loud at 5 V, and `ring` longer before its tail is choked; unpatched trigger, raises the hiss's level",
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
    #[input(
        "crossfades `_blend` from `square` into `ring`",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    mix: Option<PolySignal>,
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
    #[output(
        "blend",
        "`square` crossfaded into `ring` by `mix`",
        type = cv,
        default,
        range = (-10.0, 10.0),
    )]
    blend: PolyOutput,
    #[output(
        "square",
        "the 808-style hat; an unaccented hit peaks within ±5 V",
        type = cv,
        range = (-10.0, 10.0),
    )]
    square: PolyOutput,
    #[output(
        "ring",
        "the ring-modulated hat; an unaccented hit peaks within ±5 V",
        type = cv,
        range = (-10.0, 10.0),
    )]
    ring: PolyOutput,
}

const HI_HAT_DOCS: ModuleDocs = ModuleDocs {
    summary: "Metallic hi-hats",
    description: "Mutable Instruments Plaits' two hi-hats on one trigger: the \
                  808-style `square`, six square oscillators through a \
                  band-pass and a lopsided, saturating envelope, and the \
                  harsher `ring`, three ring-modulated square and saw pairs \
                  whose envelope chokes its tail. Leave `trigger` unpatched \
                  and they hiss steadily.",
    categories: &[ReferenceCategory::Drum],
    sections: &[],
    examples: &[
        DocExample {
            title: "Eighths",
            code: r#"
                const trigger = $`c*8`.c
                const hat = $mi.hiHat('a3', { trigger })
                hats: hat
            "#,
        },
        DocExample {
            title: "Open, noisy hat on the off-beat",
            code: r#"
                const trigger = $`~ c`.c
                const hat = $mi.hiHat('a3', {
                  trigger,
                  tone: 4,
                  decay: 4,
                  noisiness: 3.5,
                })
                hats: hat
            "#,
        },
        DocExample {
            title: "Drifting into the ring hat",
            code: r#"
                const trigger = $`c*16`.c
                const hat = $mi.hiHat('a3', {
                  trigger,
                  mix: $sine('0.1hz').range(0, 5),
                })
                hats: hat
            "#,
        },
    ],
};

/// One lane of both hats, each with its own noise.
#[derive(Clone, Copy, Default)]
struct Voice {
    strike: Strike,
    square: Lane<SquareHat>,
    ring: Lane<RingHat>,
    blend: Blend,
}

/// Seeds each field from its own rule, so a new side keeps each hat's own
/// noise for `on_patch_update` to seed.
impl crate::types::SeedSide for Voice {
    fn seed_side(&mut self, left: &Self) {
        self.strike.seed_side(&left.strike);
        self.square.seed_side(&left.square);
        self.ring.seed_side(&left.ring);
        self.blend.seed_side(&left.blend);
    }
}

#[module(name = "$mi.hiHat", docs = HI_HAT_DOCS, args(freq), has_init, patch_update)]
pub struct MiHiHat {
    outputs: MiHiHatOutputs,
    state: HatState,
    lane_state: Box<[Voice]>,
    params: MiHiHatParams,
}

impl MiHiHat {
    fn init(&mut self, sample_rate: f32) {
        self.state = HatState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        let state = &self.state;
        for (lane_index, voice) in self.lane_state[..lanes].iter_mut().enumerate() {
            let f0 = pitch(
                p.freq.get_value(lane_index),
                p.fm.value_or(lane_index, MiHiHatParams::FM_DEFAULT),
                p.fm_mode,
                state.inv_sample_rate,
            );
            let trigger = p.trigger.as_ref().map(|t| t.get_value(lane_index));
            let knob = |signal: &Option<PolySignal>, default| {
                (
                    signal.value_or(lane_index, default),
                    signal.glide_mark(lane_index),
                )
            };
            let knobs = Knobs {
                tone: knob(&p.tone, MiHiHatParams::TONE_DEFAULT),
                decay: knob(&p.decay, MiHiHatParams::DECAY_DEFAULT),
                noisiness: knob(&p.noisiness, MiHiHatParams::NOISINESS_DEFAULT),
                accent: knob(&p.accent, MiHiHatParams::ACCENT_DEFAULT),
            };
            let struck = voice.strike.read(knobs, trigger, state);
            let square = voice.square.play(f0, &struck, state);
            let ring = voice.ring.play(f0, &struck, state);
            let mix = knob(&p.mix, MiHiHatParams::MIX_DEFAULT);
            let blend = voice.blend.step(square, ring, mix, state.sample_rate);
            self.outputs.blend.set(lane_index, blend);
            self.outputs.square.set(lane_index, square);
            self.outputs.ring.set(lane_index, ring);
        }
    }
}

impl crate::types::PatchUpdateHandler for MiHiHat {
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        seed(base, 0, self.lane_state.iter_mut().map(|v| &mut v.square));
        seed(base, 1, self.lane_state.iter_mut().map(|v| &mut v.ring));
    }
}

message_handlers!(impl MiHiHat {});
