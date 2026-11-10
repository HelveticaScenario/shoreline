mod channel;
mod feedback;
mod stage;

use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::utils::{C4_HZ_F32, rng::seed_base},
    param_errors::ModuleParamErrors,
    poly::{PolyOutput, PolySignal, PolySignalExt},
};

use channel::{AllpassChannel, Controls};

const MAX_STAGES: usize = 50;

/// Largest `fadeStep`: one crossfade spanning the 1- and 50-stage chains.
const MAX_FADE_STEP: u32 = MAX_STAGES as u32 - 1;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields, validate = validate_allpass_params -> ModuleParamErrors)]
struct AllpassParams {
    #[input("the sound to smear", type = audio)]
    input: PolySignal,
    #[input(
        "the frequency the stages delay most",
        type = frequency,
        default = 0.0,
    )]
    cutoff: PolySignal,
    #[input(
        "number of stages: more stages, longer smear; fractions fade the next stage in",
        type = control,
        default = 1.0,
        range = (1.0, 50.0),
    )]
    #[deserr(default)]
    stages: Option<PolySignal>,
    #[input(
        "how sharply the smear rings at the cutoff's pitch",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    resonance: Option<PolySignal>,
    #[input(
        "repeats of the smear: rings never fade at ±5 and self-oscillate beyond; negative flips every other repeat",
        type = control,
        default = 0.0,
        range = (-6.0, 6.0),
        control_range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    feedback: Option<PolySignal>,
    #[input(
        "how `stages` moves between counts: 0 glides smoothly from one to the next; above 0 it blends two chains this many stages apart, which cuts phaser-like notches",
        range = (0, 49)
    )]
    #[serde(default)]
    #[deserr(default)]
    fade_step: u32,
}

fn validate_allpass_params(
    params: AllpassParams,
    _location: deserr::ValuePointerRef,
) -> Result<AllpassParams, ModuleParamErrors> {
    if params.fade_step > MAX_FADE_STEP {
        let mut err = ModuleParamErrors::default();
        err.add(
            "fadeStep".to_string(),
            format!("must be from 0 to {MAX_FADE_STEP}"),
        );
        return Err(err);
    }
    Ok(params)
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AllpassOutputs {
    #[output("output", "phase-shifted signal", type = cv, default)]
    sample: PolyOutput,
}

const APF_DOCS: ModuleDocs = ModuleDocs {
    summary: "Stacked allpass smear",
    description: "Up to 50 allpass stages that smear transients into chirps around the cutoff. \
                  Resonance and feedback make the smear ring at a pitch, and mixed with the dry \
                  signal it notches like a phaser.",
    categories: &[ModuleCategory::Filter, ModuleCategory::Effect],
    sections: &[],
    examples: &[
        DocExample {
            title: "A click smeared into a chirp",
            code: "$pPulse($clock[0]).$.apf('c3', { stages: 50 }).out()",
        },
        DocExample {
            title: "Ringing, pitched smear with repeats",
            code: r#"
                $pPulse($clock[0])
                  .$.apf('g3', { stages: 32, resonance: 3, feedback: 3 })
                  .out()
            "#,
        },
        DocExample {
            title: "Four-stage phaser",
            code: r#"
                $saw('c2')
                  .$m.apf(2.5, $sine('0.2hz').range(-1, 3), {
                    stages: 4,
                    resonance: 2,
                    feedback: 3,
                  })
                  .out()
            "#,
        },
    ],
};

#[module(
    name = "$apf",
    docs = APF_DOCS,
    args(input, cutoff),
    has_init,
    block
)]
pub struct Allpass {
    outputs: AllpassOutputs,
    state: AllpassState,
    channel_state: Box<[AllpassChannel]>,
    params: AllpassParams,
}

#[derive(Default)]
struct AllpassState {
    /// Converts a V/Oct cutoff to octaves relative to the sample rate.
    log2_c4_over_fs: f32,
}

impl Allpass {
    fn init(&mut self, sample_rate: f32) {
        self.state.log2_c4_over_fs = (C4_HZ_F32 / sample_rate).log2();
        let base = seed_base(self);
        for (ch, state) in self.channel_state.iter_mut().enumerate() {
            state.seed_noise(base, ch);
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let fade_step = self.params.fade_step as usize;
        let p = &self.params;
        for (ch, state) in self.channel_state.iter_mut().enumerate() {
            let controls = Controls {
                cutoff: p.cutoff.get_value(ch),
                stages: p.stages.value_or(ch, AllpassParams::STAGES_DEFAULT),
                resonance: p.resonance.value_or(ch, AllpassParams::RESONANCE_DEFAULT),
                feedback: p.feedback.value_or(ch, AllpassParams::FEEDBACK_DEFAULT),
            };
            let y = state.step(
                p.input.get_value(ch),
                controls,
                fade_step,
                self.state.log2_c4_over_fs,
            );
            self.outputs.sample.set(ch, y);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut AllpassBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let fade_step = self.params.fade_step as usize;
        let mut scratch = Default::default();
        let inputs = self
            .params
            .block_inputs(range.clone(), self.channel_count(), &mut scratch)?;
        for (ch, state) in self.channel_state.iter_mut().enumerate() {
            for i in range.clone() {
                let controls = Controls {
                    cutoff: inputs.cutoff.row(i)[ch],
                    stages: inputs.stages.row(i)[ch],
                    resonance: inputs.resonance.row(i)[ch],
                    feedback: inputs.feedback.row(i)[ch],
                };
                let y = state.step(
                    inputs.input.row(i)[ch],
                    controls,
                    fade_step,
                    self.state.log2_c4_over_fs,
                );
                outputs.sample.slot_mut(i)[ch] = y;
            }
        }
        Some(())
    }
}

message_handlers!(impl Allpass {});

#[cfg(test)]
mod tests;
