//! Groove: a periodic, monotonic time map applied to a playhead.
//!
//! Grooves reach sequencers as an offset (grooved minus straight position): a
//! note starts when the grooved position reaches it, so grid-aligned notes
//! start at their shifted times and off-grid notes slide proportionally inside
//! their step, while note lengths stay measured in straight time.

mod follower;
mod template;

pub use follower::{GrooveFollower, GrooveHold, before_first_step};
pub use template::{GrooveTemplate, fill_groove_inputs};

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{MonoSignal, param_errors::ModuleParamErrors};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields, validate = validate_groove_params -> ModuleParamErrors)]
struct GrooveParams {
    #[input("timing offsets, velocities and step grid of the groove")]
    template: GrooveTemplate,
    #[input(
        "straight playhead to groove, in cycles; follows the global clock (without its groove) by default",
        type = control,
        range = (0.0, ..),
        control_range = (0.0, 1.0),
    )]
    #[default_connection(module = RootClock, port = "playhead", channels = [0, 1])]
    #[deserr(default)]
    playhead: Option<MonoSignal>,
}

fn validate_groove_params(
    params: GrooveParams,
    _location: deserr::ValuePointerRef,
) -> Result<GrooveParams, ModuleParamErrors> {
    if let Err(msg) = params.template.validate() {
        let mut err = ModuleParamErrors::default();
        err.add("template".to_string(), msg);
        return Err(err);
    }
    Ok(params)
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GrooveOutputs {
    #[output(
        "offset",
        "how far the groove shifts the playhead, in cycles; pass to a sequencer's `groove`",
        default
    )]
    offset: f32,
    #[output("velocity", "velocity of the current groove step; pass to a sequencer's `velocity`", range = (0.0, 5.0))]
    velocity: f32,
    #[output("trigger", "fires at the start of each grooved step", range = (0.0, 5.0))]
    trigger: f32,
}

const GROOVE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Timing and velocity groove",
    description: "Pushes steps of the grid early or late and gives each a velocity, for swing \
                  and shuffle on a single sequencer. Every sequencer follows the global groove \
                  from `$setGroove` by default; pass this module and its `velocity` to one to \
                  give it a feel of its own. Notes keep their written lengths.",
    categories: &[ModuleCategory::Sequencer],
    sections: &[],
    examples: &[DocExample {
        title: "Shuffled eighths with soft offbeats",
        code: "const g = $groove({ timing: [0, 0.3], velocity: [1, 0.6], grid: 8 })\n\
               const seq = $cycle($p('c4 e4 g4 c5 g4 e4 c4 g3'), { groove: g, velocity: g.velocity })\n\
               $sine(seq).amp($adsr(seq.gate)).amp(seq.velocity).out()",
    }],
};

#[module(name = "$groove", docs = GROOVE_DOCS, args(template))]
pub struct Groove {
    outputs: GrooveOutputs,
    params: GrooveParams,
    state: GrooveFollower,
}

impl Groove {
    fn update(&mut self, sample_rate: f32) {
        let straight = self
            .params
            .playhead
            .as_ref()
            .map_or(0.0, MonoSignal::get_value_f64);
        let frame = self
            .state
            .advance(&self.params.template, straight, sample_rate);
        self.outputs.offset = frame.offset;
        self.outputs.velocity = frame.velocity;
        self.outputs.trigger = frame.trigger;
    }
}

message_handlers!(impl Groove {});
