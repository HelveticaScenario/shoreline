use std::ops::Range;

use super::*;
use crate::dsp::drums::input::{Cached, Held, Hold, Input, Live, each_slot};
use crate::dsp::drums::tr909::circuit::TRIG_SECS;
use crate::dsp::drums::voice::{Timing, knob, rails};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct Sd909Params {
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
        "pitch; stock is ≈148 Hz",
        type = pitch,
        default = -0.8243,
    )]
    #[deserr(default)]
    freq: Option<PolySignal>,
    #[input(
        "how long the noise rings on, from a short snap to a long hiss",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "how loud the noise is",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    snappy: Option<PolySignal>,
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
struct Sd909Outputs {
    #[output("output", "drum output; an unaccented hit peaks near ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const SD909_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-909 snare drum",
    description: "The 909's snare: a drum tone that bends up at the strike under a burst \
                  of noise, which `tone` stretches from a tight crack to a long hiss.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Backbeat doubled on four",
        code: r#"
            $tr909.kick($p('c*4')).out()
            $tr909.snare($p('~ c ~ [c c]'), { snappy: 4 }).out()
        "#,
    }],
};

#[module(
    name = "$tr909.snare",
    docs = SD909_DOCS,
    has_init,
    patch_update,
    args(trigger),
    block
)]
pub struct Sd909 {
    outputs: Sd909Outputs,
    params: Sd909Params,
    state: SdState,
    lane_state: Box<[Lane]>,
}

#[derive(Default)]
struct SdState {
    hold: Hold<1, 3>,
    shared: SnareShared,
    groups: Groups<SnareDrum>,
}

#[derive(Clone, Copy, Default)]
struct Lane {
    voices: Voices<Snare>,
    noise: Noise,
    rate: Cached<f32, f32>,
    /// C67's per-sample fall through the tone pot at its knob's travel.
    tone_fall: Cached<f32, f32>,
}

impl Sd909 {
    fn init(&mut self, sample_rate: f32) {
        let dt = 1.0 / sample_rate;
        self.state.groups = Groups::new();
        let sh = &mut self.state.shared;
        sh.timing = Timing::new(TRIG_SECS, sample_rate);
        sh.dt = dt;
        sh.noise_rate = Noise::rate(sample_rate);
        let mut injected = [0.0f32; NOISE_UNKNOWNS];
        injected[NOISE_IN] = NOISE_MEAN * STIFF;
        sh.noise_rest = noise_network(DC_STEP).settle(0, &injected);
        sh.noise = noise_network(dt);
        sh.reset1 = (-dt / VCO1_RESET).exp();
        sh.reset2 = (-dt / VCO2_RESET).exp();
        let g_tune = 1.0 / (R248 + R249);
        sh.c66_charge = (-(1.0 / R247 + g_tune) * dt / C66).exp();
        sh.c66_bleed = (-g_tune * dt / C66).exp();
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        let inputs = [
            Live::new(&p.trigger),
            Live::or(&p.accent, Sd909Params::ACCENT_DEFAULT),
            Live::or(&p.freq, Sd909Params::FREQ_DEFAULT),
            Live::or(&p.tone, Sd909Params::TONE_DEFAULT),
            Live::or(&p.snappy, Sd909Params::SNAPPY_DEFAULT),
        ];
        let out = &mut self.outputs.sample;
        self.state.tick(
            &mut self.lane_state[..lanes],
            p.restart,
            p.hold,
            inputs,
            |lane, v| out.set(lane, v),
        );
    }

    fn process_block(
        &mut self,
        outputs: &mut Sd909BlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let Sd909ParamsBlockInputs {
            trigger,
            accent,
            freq,
            tone,
            snappy,
        } = p.block_inputs(range.clone(), lanes, &mut scratch)?;
        let lane_state = &mut self.lane_state[..lanes];
        each_slot(
            &mut outputs.sample,
            range,
            [trigger, accent, freq, tone, snappy],
            |inputs, mut slot| {
                self.state
                    .tick(lane_state, p.restart, p.hold, inputs, |lane, v| {
                        slot.set(lane, v)
                    })
            },
        );
        Some(())
    }
}

impl SdState {
    /// Runs one sample of every lane on its `[trigger, accent, freq, tone,
    /// snappy]`, handing `out` each one's output.
    #[inline(always)]
    fn tick<I: Input>(
        &mut self,
        lanes: &mut [Lane],
        restart: bool,
        hold: bool,
        [trigger, accent, freq, tone, snappy]: [I; 5],
        mut out: impl FnMut(usize, f32),
    ) {
        let [freq, tone, snappy] =
            self.hold
                .apply(hold, lanes.len(), [trigger], [freq, tone, snappy]);
        let trigger = Held::Live(trigger);
        let run = Run {
            accent,
            restart,
            fade_samples: self.shared.timing.fade_samples,
            shared: &self.shared,
        };
        let inputs = SdInputs {
            trigger,
            freq,
            tone,
            snappy,
            noise_rate: self.shared.noise_rate,
            dt: self.shared.dt,
        };
        self.groups.run(&inputs, lanes, &run, |lane, v| {
            out(lane, rails(v * OUTPUT_GAIN))
        });
    }
}

/// The inputs every lane of the snare reads.
struct SdInputs<I> {
    trigger: I,
    freq: I,
    tone: I,
    snappy: I,
    noise_rate: f32,
    dt: f32,
}

impl<I: Input> LaneInputs<SnareDrum> for SdInputs<I> {
    type Lane = Lane;

    fn voices(lane: &mut Lane) -> &mut Voices<Snare> {
        &mut lane.voices
    }

    #[inline(always)]
    fn inputs(&self, lane_index: usize, lane: &mut Lane) -> (f32, SnareControls) {
        let tone_fall = lane.tone_fall.get(knob(self.tone.at(lane_index)), |k| {
            (-(1.0 / (R254 + VR7 * k)) * self.dt / C67).exp()
        });
        let controls = SnareControls {
            rate: lane.rate.get(self.freq.at(lane_index), |f| {
                (f - Sd909Params::FREQ_DEFAULT).exp2()
            }),
            tone_fall,
            snappy: VR9 * knob(self.snappy.at(lane_index)) as f64,
            noise: lane.noise.next(self.noise_rate),
        };
        (self.trigger.at(lane_index), controls)
    }
}

impl crate::types::PatchUpdateHandler for Sd909 {
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (lane, cs) in self.lane_state.iter_mut().enumerate() {
            cs.noise.seed(base, lane);
        }
        let count = self.lane_count();
        self.state
            .groups
            .layout::<SdInputs<&[f32]>>(&mut self.lane_state, count);
    }
}

message_handlers!(impl Sd909 {});
