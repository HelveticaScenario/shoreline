use std::ops::Range;

use super::*;
use crate::dsp::drums::input::{Cached, Held, Hold, Input, Live, each_slot};
use crate::dsp::drums::tr909::circuit::{TRIG_SECS, tapered_pot};
use crate::dsp::drums::voice::{Timing, knob, rails};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct Bd909Params {
    #[input("rising edges strike the drum", type = trig)]
    trigger: PolySignal,
    #[input(
        "how hard the hit lands, read at each strike: up to twice as loud at 5 V",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input(
        "resting pitch, the sweep following; stock is ≈56 Hz",
        type = pitch,
        default = -2.2235,
    )]
    #[deserr(default)]
    freq: Option<PolySignal>,
    #[input(
        "how long the pitch takes to fall to rest; the 909's tune knob",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    sweep: Option<PolySignal>,
    #[input(
        "how loud the click is",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    attack: Option<PolySignal>,
    #[input(
        "how long the drum booms",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "start every hit from rest, fading out one still sounding over 2 ms; off, each hit builds on the last, as on the 909"
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
struct Bd909Outputs {
    #[output("output", "drum output; an unaccented hit peaks near ±5 V", default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const BD909_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-909 bass drum",
    description: "The 909's punchy kick: a rounded tone that sweeps down to a low rest, \
                  with a click and a puff of noise at the attack. The backbone of house \
                  and techno.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Four on the floor",
        code: "$tr909.kick($cycle($p('c*4')).trig, { decay: 4 }).out()",
    }],
};

#[module(
    name = "$tr909.kick",
    docs = BD909_DOCS,
    has_init,
    patch_update,
    args(trigger),
    block
)]
pub struct Bd909 {
    outputs: Bd909Outputs,
    params: Bd909Params,
    state: BdState,
    channel_state: Box<[Channel]>,
}

#[derive(Default)]
struct BdState {
    hold: Hold<1, 4>,
    shared: KickShared,
    groups: Groups<KickDrum>,
}

#[derive(Clone, Copy, Default)]
struct Channel {
    voices: Voices<Kick>,
    noise: Noise,
    rate: Cached<f32, f32>,
    /// The sweep and decay pots' conductances at their knobs' travel.
    sweep: Cached<f32, f32>,
    decay: Cached<f32, f32>,
}

impl Bd909 {
    fn init(&mut self, sample_rate: f32) {
        let dt = 1.0 / sample_rate;
        self.state.groups = Groups::new();
        let sh = &mut self.state.shared;
        sh.timing = Timing::new(TRIG_SECS, sample_rate);
        sh.dt = dt;
        sh.noise_rate = Noise::rate(sample_rate);
        sh.envelope = envelope_network(dt);
        let mut injected = [0.0f32; ATTACK_UNKNOWNS];
        injected[NOISE_IN] = NOISE_MEAN * STIFF;
        injected[PE] = -RAIL / PE_PULL;
        sh.attack_rest = attack_network(DC_STEP).settle(1 << Q8_ON, &injected);
        sh.attack = attack_network(dt);
        sh.sync_coeff = (-dt / (R51 * C14)).exp();
        sh.c7_charge = (-dt / (R20 * C7)).exp();
        sh.c7_bleed = (-dt / (R18 * C7)).exp();
        sh.c12_charge = (-(1.0 / R41 + 1.0 / R493) * dt / C12).exp();
        sh.c12_bleed = (-dt / (R493 * C12)).exp();
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let inputs = [
            Live::new(&p.trigger),
            Live::or(&p.accent, Bd909Params::ACCENT_DEFAULT),
            Live::or(&p.freq, Bd909Params::FREQ_DEFAULT),
            Live::or(&p.sweep, Bd909Params::SWEEP_DEFAULT),
            Live::or(&p.decay, Bd909Params::DECAY_DEFAULT),
            Live::or(&p.attack, Bd909Params::ATTACK_DEFAULT),
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
        outputs: &mut Bd909BlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let Bd909ParamsBlockInputs {
            trigger,
            accent,
            freq,
            sweep,
            decay,
            attack,
        } = p.block_inputs(range.clone(), channels, &mut scratch)?;
        let channel_state = &mut self.channel_state[..channels];
        each_slot(
            &mut outputs.sample,
            range,
            [trigger, accent, freq, sweep, decay, attack],
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
    /// Runs one sample of every channel on its `[trigger, accent, freq,
    /// sweep, decay, attack]`, handing `out` each one's output.
    #[inline(always)]
    fn tick<I: Input>(
        &mut self,
        channels: &mut [Channel],
        restart: bool,
        hold: bool,
        [trigger, accent, freq, sweep, decay, attack]: [I; 6],
        mut out: impl FnMut(usize, f32),
    ) {
        let [freq, sweep, decay, attack] = self.hold.apply(
            hold,
            channels.len(),
            [trigger],
            [freq, sweep, decay, attack],
        );
        let trigger = Held::Live(trigger);
        let run = Run {
            accent,
            restart,
            fade_samples: self.shared.timing.fade_samples,
            shared: &self.shared,
        };
        let inputs = BdInputs {
            trigger,
            freq,
            sweep,
            decay,
            attack,
            noise_rate: self.shared.noise_rate,
        };
        self.groups.run(&inputs, channels, &run, |ch, v| {
            out(ch, rails(v * OUTPUT_GAIN))
        });
    }
}

/// The inputs every channel of the bass drum reads.
struct BdInputs<I> {
    trigger: I,
    freq: I,
    sweep: I,
    decay: I,
    attack: I,
    noise_rate: f32,
}

impl<I: Input> ChannelInputs<KickDrum> for BdInputs<I> {
    type Channel = Channel;

    fn voices(channel: &mut Channel) -> &mut Voices<Kick> {
        &mut channel.voices
    }

    #[inline(always)]
    fn inputs(&self, ch: usize, cs: &mut Channel) -> (f32, KickControls) {
        let controls = KickControls {
            rate: cs
                .rate
                .get(self.freq.at(ch), |f| (f - Bd909Params::FREQ_DEFAULT).exp2()),
            sweep: cs
                .sweep
                .get(knob(self.sweep.at(ch)), |k| tapered_pot(k, R57, VR2)),
            decay: cs
                .decay
                .get(knob(self.decay.at(ch)), |k| tapered_pot(k, R58, VR5)),
            attack: VR3 * (1.0 - knob(self.attack.at(ch))),
            noise: cs.noise.next(self.noise_rate),
        };
        (self.trigger.at(ch), controls)
    }
}

impl crate::types::PatchUpdateHandler for Bd909 {
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, cs) in self.channel_state.iter_mut().enumerate() {
            cs.noise.seed(base, ch);
        }
        let count = self.channel_count();
        self.state
            .groups
            .layout::<BdInputs<&[f32]>>(&mut self.channel_state, count);
    }
}

message_handlers!(impl Bd909 {});
