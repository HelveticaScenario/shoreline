use std::ops::Range;

use super::*;
use crate::dsp::drums::input::{Input, Live, each_slot};
use crate::dsp::drums::tr808::circuit::PULSE_SECS;
use crate::dsp::drums::voice::{Timing, rails};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct Ma808Params {
    #[input("rising edges shake the maracas", type = trig)]
    trigger: PolySignal,
    #[input(
        "how hard the shake lands, read at each strike: up to twice as loud at 5 V, and drawn out",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input(
        "start every shake from rest, fading out one still sounding over 2 ms; off, each shake runs into the last, as on the 808"
    )]
    #[serde(default)]
    #[deserr(default)]
    restart: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Ma808Outputs {
    #[output("output", "drum output; an unaccented hit peaks near ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const MA808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 maracas",
    description: "The 808's maracas: a short shake of high, hissing noise, for a light \
                  shuffle on top of a beat.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Sixteenth shaker",
        code: r#"
            $tr808.kick($`c*4`).out()
            $tr808.maracas($`c*16`).out()
        "#,
    }],
};

#[module(
    name = "$tr808.maracas",
    docs = MA808_DOCS,
    has_init,
    patch_update,
    args(trigger),
    block
)]
pub struct Ma808 {
    outputs: Ma808Outputs,
    params: Ma808Params,
    state: MaState,
    lane_state: Box<[Lane]>,
}

#[derive(Default)]
struct MaState {
    shared: ShakeShared,
}

#[derive(Clone, Copy, Default)]
struct Lane {
    voices: Voices<Shake>,
    noise: WhiteNoise,
}

/// Iterations that settle the switches at DC.
const DC_ITERATIONS: usize = 20;

impl Ma808 {
    fn init(&mut self, sample_rate: f32) {
        let sh = &mut self.state.shared;
        sh.envelope = envelope_network(DC_STEP);
        sh.noise = noise_network(DC_STEP);
        let mut rest = Shake {
            primed: true,
            ..Shake::default()
        };
        for _ in 0..DC_ITERATIONS {
            rest.advance(sh, 0.0, 0.0);
        }
        rest.quiet = Quiet::default();
        sh.rest = rest;
        let dt = 1.0 / sample_rate;
        sh.timing = Timing::new(PULSE_SECS, sample_rate);
        sh.noise_scale = WhiteNoise::scale(sample_rate);
        sh.envelope = envelope_network(dt);
        sh.noise = noise_network(dt);
        sh.c130_coeff = one_pole_coeff(rc_hz(R335, C130), sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        let inputs = [
            Live::new(&p.trigger),
            Live::or(&p.accent, Ma808Params::ACCENT_DEFAULT),
        ];
        let out = &mut self.outputs.sample;
        tick(
            &self.state,
            &mut self.lane_state[..lanes],
            p.restart,
            inputs,
            |lane, v| out.set(lane, v),
        );
    }

    fn process_block(
        &mut self,
        outputs: &mut Ma808BlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let Ma808ParamsBlockInputs { trigger, accent } =
            p.block_inputs(range.clone(), lanes, &mut scratch)?;
        let lane_state = &mut self.lane_state[..lanes];
        each_slot(
            &mut outputs.sample,
            range,
            [trigger, accent],
            |inputs, mut slot| {
                tick(&self.state, lane_state, p.restart, inputs, |lane, v| {
                    slot.set(lane, v)
                })
            },
        );
        Some(())
    }
}

/// Runs one sample of every lane on its `[trigger, accent]`, handing
/// `out` each one's output.
#[inline(always)]
fn tick<I: Input>(
    state: &MaState,
    lanes: &mut [Lane],
    restart: bool,
    [trigger, accent]: [I; 2],
    mut out: impl FnMut(usize, f32),
) {
    let sh = &state.shared;
    for (lane, cs) in lanes.iter_mut().enumerate() {
        let white = cs.noise.next(sh.noise_scale);
        let v = cs.voices.process(
            trigger.at(lane),
            accent,
            lane,
            restart,
            sh.timing.fade_samples,
            sh,
            &white,
        );
        out(lane, rails(v * OUTPUT_GAIN));
    }
}

impl crate::types::PatchUpdateHandler for Ma808 {
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (lane, cs) in self.lane_state.iter_mut().enumerate() {
            cs.noise.seed(base, lane);
        }
    }
}

message_handlers!(impl Ma808 {});
