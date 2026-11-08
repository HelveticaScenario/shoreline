use std::ops::Range;

use super::*;
use crate::dsp::drums::input::{Cached, Held, Hold, Input, Live, each_slot};
use crate::dsp::drums::tr808::circuit::PULSE_SECS;
use crate::dsp::drums::voice::{Timing, knob, rails};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct Sd808Params {
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
        "blend from the low tone to the high one",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "how loud and long the noise rattles over the tones",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    snappy: Option<PolySignal>,
    #[input(
        "pitch of the low tone, the high one following; stock is ≈250 Hz, and the hit gets louder or quieter far from it",
        type = pitch,
        default = -0.0678,
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
struct Sd808Outputs {
    #[output("output", "drum output; an unaccented hit peaks near ±5 V", default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

#[derive(Clone, Copy, Default)]
struct Channel {
    voices: Voices<Snare>,
    noise: WhiteNoise,
    /// The resonators tuned for the channel's `freq`.
    tuning: Cached<f32, Tuning>,
}

const SD808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 snare drum",
    description: "The 808's snare: two drum tones under a burst of bright noise, from a \
                  dull thump to a crisp, rattling snap.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Electro backbeat",
        code: "$tr808.kick($cycle($p('c ~ ~ c ~ ~ c ~')).trig, { decay: 3 }).out()\n\
               $tr808.snare($cycle($p('~ c ~ c')).trig, { snappy: 4 }).out()",
    }],
};

#[module(
    name = "$tr808.snare",
    docs = SD808_DOCS,
    has_init,
    patch_update,
    args(trigger),
    block
)]
pub struct Sd808 {
    outputs: Sd808Outputs,
    params: Sd808Params,
    state: SdState,
    channel_state: Box<[Channel]>,
}

/// The snare's per-sample-rate constants and its channel groups.
#[derive(Default)]
struct SdState {
    hold: Hold<1, 3>,
    shared: SnareShared,
    groups: Groups<SnareDrum>,
}

impl Sd808 {
    fn init(&mut self, sample_rate: f32) {
        self.state.groups = Groups::new();
        let sh = &mut self.state.shared;
        let dt = 1.0 / sample_rate;
        sh.dt = dt;
        sh.timing = Timing::new(PULSE_SECS, sample_rate);
        sh.rest_hz = LOW.rest_hz(1.0 / R196);
        sh.low_decay = T60_TO_RATE / LOW.decay_rate();
        sh.high_decay = T60_TO_RATE / HIGH.decay_rate();
        sh.noise_scale = WhiteNoise::scale(sample_rate);
        let tau = C57 * R189 * R190 / (R189 + R190);
        sh.c57_mul = (-dt / tau).exp();
        sh.c57_avg = tau / dt * (1.0 - sh.c57_mul);
        // A one-pole low-pass `y += c·(x − y)` passes c / (2 − c) of white
        // noise's power; the generator's uniform samples have rms 1/√3.
        sh.chop_coeff = 1.0 - (-std::f32::consts::TAU * CHOP_HZ * dt).exp();
        sh.chop_norm =
            3f32.sqrt() / sh.noise_scale * ((2.0 - sh.chop_coeff) / sh.chop_coeff).sqrt();
        sh.snap_hp = Biquad::sallen_key_high(C66, C67, R201, R202, sample_rate);
        sh.snap_couple_coeff = one_pole_coeff(rc_hz(R205, C69), sample_rate);
        sh.out_lp_coeff = one_pole_coeff(rc_hz(R_LEVEL, C70), sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let inputs = [
            Live::new(&p.trigger),
            Live::or(&p.accent, Sd808Params::ACCENT_DEFAULT),
            Live::or(&p.freq, Sd808Params::FREQ_DEFAULT),
            Live::or(&p.tone, Sd808Params::TONE_DEFAULT),
            Live::or(&p.snappy, Sd808Params::SNAPPY_DEFAULT),
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
        outputs: &mut Sd808BlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let Sd808ParamsBlockInputs {
            trigger,
            accent,
            freq,
            tone,
            snappy,
        } = p.block_inputs(range.clone(), channels, &mut scratch)?;
        let channel_state = &mut self.channel_state[..channels];
        each_slot(
            &mut outputs.sample,
            range,
            [trigger, accent, freq, tone, snappy],
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

impl SdState {
    /// Runs one sample of every channel on its `[trigger, accent, freq, tone,
    /// snappy]`, handing `out` each one's output.
    #[inline(always)]
    fn tick<I: Input>(
        &mut self,
        channels: &mut [Channel],
        restart: bool,
        hold: bool,
        [trigger, accent, freq, tone, snappy]: [I; 5],
        mut out: impl FnMut(usize, f32),
    ) {
        let [freq, tone, snappy] =
            self.hold
                .apply(hold, channels.len(), [trigger], [freq, tone, snappy]);
        let trigger = Held::Live(trigger);
        let run = Run {
            accent,
            restart,
            fade_samples: self.shared.timing.fade_samples,
            shared: &self.shared,
        };
        let inputs = SdInputs {
            sh: &self.shared,
            trigger,
            freq,
            tone,
            snappy,
        };
        self.groups.run(&inputs, channels, &run, |ch, v| {
            out(ch, rails(v * OUTPUT_GAIN))
        });
    }
}

/// The inputs every channel of the snare reads.
struct SdInputs<'a, I> {
    sh: &'a SnareShared,
    trigger: I,
    freq: I,
    tone: I,
    snappy: I,
}

impl<I: Input> ChannelInputs<SnareDrum> for SdInputs<'_, I> {
    type Channel = Channel;

    fn voices(channel: &mut Channel) -> &mut Voices<Snare> {
        &mut channel.voices
    }

    #[inline(always)]
    fn inputs(&self, ch: usize, cs: &mut Channel) -> (f32, SnareControls) {
        let tuning = cs
            .tuning
            .get(self.freq.at(ch), |freq| Tuning::new(self.sh, freq));
        let controls = SnareControls {
            low: tuning.low,
            high: tuning.high,
            tone: knob(self.tone.at(ch)),
            snappy: knob(self.snappy.at(ch)),
            white: cs.noise.next(self.sh.noise_scale),
            level: tuning.level,
        };
        (self.trigger.at(ch), controls)
    }
}

impl crate::types::PatchUpdateHandler for Sd808 {
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, cs) in self.channel_state.iter_mut().enumerate() {
            cs.noise.seed(base, ch);
        }
        let count = self.channel_count();
        self.state
            .groups
            .layout::<SdInputs<&[f32]>>(&mut self.channel_state, count);
    }
}

message_handlers!(impl Sd808 {});
