use std::ops::Range;

use super::*;
use crate::dsp::drums::input::{Cached, Held, Hold, Input, Live, each_slot};
use crate::dsp::drums::tr808::circuit::PULSE_SECS;
use crate::dsp::drums::voice::{Timing, rails};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct Bd808Params {
    #[input("rising edges strike the drum", type = trig)]
    trigger: PolySignal,
    #[input(
        "how hard the hit lands, read at each strike: up to twice as loud at 5 V, with a deeper pitch drop",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input(
        "brightness of the attack, from a dull thud to a bright click",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "how long the drum rings, from ≈0.09 s to ≈3.8 s; ≈0.6 s at 2.5",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "pitch; stock is ≈49.5 Hz; its level shifts as you tune far from stock",
        type = pitch,
        default = -2.4035,
    )]
    #[deserr(default)]
    freq: Option<PolySignal>,
    #[input(
        "start every hit from rest, fading out one still sounding over 2 ms; off, each hit builds on the last, as on the 808"
    )]
    #[serde(default)]
    #[deserr(default)]
    restart: bool,
    #[input(
        "sample the controls on each strike and hold them until the next, as accent always is, so changes reach only the hits after them"
    )]
    #[serde(default)]
    #[deserr(default)]
    hold: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Bd808Outputs {
    #[output("output", "drum output; an unaccented hit peaks near ±5 V", default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const BD808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 bass drum",
    description: "The 808's deep, booming kick: a round, low ring with a click at the \
                  attack and a pitch that sighs down as it fades. Long decays turn it \
                  into a tuned sub bass. As on the 808, a hit that lands on a \
                  still-ringing one builds on it, so long decays at fast tempos swing in \
                  level from hit to hit.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[
        DocExample {
            title: "Booming four on the floor",
            code: "$tr808.kick($cycle($p('c*4')).trig, { decay: 4 }).out()",
        },
        DocExample {
            title: "Tuned 808 bassline",
            code: r#"
                const bass = $cycle($p('g1 ~ ~ g1 ~ ~ bb1 c2'))
                $tr808.kick(bass.trig, { freq: bass, decay: 4.5 }).out()
            "#,
        },
    ],
};

#[module(
    name = "$tr808.kick",
    docs = BD808_DOCS,
    has_init,
    patch_update,
    args(trigger),
    block
)]
pub struct Bd808 {
    outputs: Bd808Outputs,
    params: Bd808Params,
    state: BdState,
    channel_state: Box<[Channel]>,
}

/// The bass drum's per-sample-rate constants and its channel groups.
#[derive(Default)]
struct BdState {
    hold: Hold<1, 3>,
    shared: KickShared,
    groups: Groups<KickDrum>,
}

#[derive(Clone, Copy, Default)]
struct Channel {
    voices: Voices<Kick>,
    /// The bridged-T for the channel's `freq` and `decay`.
    network: Cached<(f32, f32), Network>,
    /// The tone low-pass coefficient for its `tone`.
    tone: Cached<f32, f32>,
}

impl Bd808 {
    fn init(&mut self, sample_rate: f32) {
        self.state.groups = Groups::new();
        let sh = &mut self.state.shared;
        let dt = 1.0 / sample_rate;
        sh.timing = Timing::new(PULSE_SECS, sample_rate);
        sh.dt = dt;
        sh.sample_rate = sample_rate;
        let shaper_tau = C40 * R162 * R163 / (R162 + R163);
        sh.shaper_mul = (-dt / shaper_tau).exp();
        sh.c38_coeff = 1.0 - (-dt / R156_C38).exp();
        sh.c39_step = dt / (R161 * C39);
        sh.level_hp_coeff = one_pole_coeff(LEVEL_HP_HZ, sample_rate);
        sh.output_hp_coeff = one_pole_coeff(OUTPUT_HP_HZ, sample_rate);
        let rest_r = 1.0 / (1.0 / R161 + 1.0 / R170 + 1.0 / (R165 + R166));
        sh.rest_hz = 1.0 / (std::f32::consts::TAU * (rest_r * R167).sqrt() * C41_42);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let inputs = [
            Live::new(&p.trigger),
            Live::or(&p.accent, Bd808Params::ACCENT_DEFAULT),
            Live::or(&p.freq, Bd808Params::FREQ_DEFAULT),
            Live::or(&p.decay, Bd808Params::DECAY_DEFAULT),
            Live::or(&p.tone, Bd808Params::TONE_DEFAULT),
        ];
        let out = &mut self.outputs.sample;
        self.state.tick(
            &mut self.channel_state[..channels],
            p.restart,
            p.hold,
            inputs,
            |ch, v| out.set(ch, v),
        );
    }

    fn process_block(
        &mut self,
        outputs: &mut Bd808BlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let Bd808ParamsBlockInputs {
            trigger,
            accent,
            freq,
            decay,
            tone,
        } = p.block_inputs(range.clone(), channels, &mut scratch)?;
        let channel_state = &mut self.channel_state[..channels];
        each_slot(
            &mut outputs.sample,
            range,
            [trigger, accent, freq, decay, tone],
            |inputs, mut slot| {
                self.state
                    .tick(channel_state, p.restart, p.hold, inputs, |ch, v| {
                        slot.set(ch, v)
                    })
            },
        );
        Some(())
    }
}

impl BdState {
    /// Runs one sample of every channel on its `[trigger, accent, freq, decay,
    /// tone]`, handing `out` each one's output.
    #[inline(always)]
    fn tick<I: Input>(
        &mut self,
        channels: &mut [Channel],
        restart: bool,
        hold: bool,
        [trigger, accent, freq, decay, tone]: [I; 5],
        mut out: impl FnMut(usize, f32),
    ) {
        let [freq, decay, tone] =
            self.hold
                .apply(hold, channels.len(), [trigger], [freq, decay, tone]);
        let trigger = Held::Live(trigger);
        let run = Run {
            accent,
            restart,
            fade_samples: self.shared.timing.fade_samples,
            shared: &self.shared,
        };
        let inputs = BdInputs {
            sh: &self.shared,
            trigger,
            freq,
            decay,
            tone,
        };
        self.groups.run(&inputs, channels, &run, |ch, v| {
            out(ch, rails(v * OUTPUT_GAIN))
        });
    }
}

/// The inputs every channel of the bass drum reads.
struct BdInputs<'a, I> {
    sh: &'a KickShared,
    trigger: I,
    freq: I,
    decay: I,
    tone: I,
}

impl<I: Input> ChannelInputs<KickDrum> for BdInputs<'_, I> {
    type Channel = Channel;

    fn voices(channel: &mut Channel) -> &mut Voices<Kick> {
        &mut channel.voices
    }

    #[inline(always)]
    fn inputs(&self, ch: usize, cs: &mut Channel) -> (f32, KickControls) {
        let network = cs
            .network
            .get((self.freq.at(ch), self.decay.at(ch)), |(freq, decay)| {
                Network::new(freq, decay, self.sh.rest_hz)
            });
        let tone_coeff = cs.tone.get(self.tone.at(ch), |tone| {
            one_pole_coeff(tone_hz(tone), self.sh.sample_rate)
        });
        let controls = KickControls {
            c: network.c,
            a: network.a,
            g: network.g,
            tone_coeff,
            level: network.level,
        };
        (self.trigger.at(ch), controls)
    }
}

impl crate::types::PatchUpdateHandler for Bd808 {
    fn on_patch_update(&mut self) {
        let count = self.channel_count();
        self.state
            .groups
            .layout::<BdInputs<&[f32]>>(&mut self.channel_state, count);
    }
}

message_handlers!(impl Bd808 {});
