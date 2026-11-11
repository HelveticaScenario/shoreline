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
use crate::module_docs::{DocExample, DocSection, DocTable, ModuleDocs};
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
        type = cv,
        default
    )]
    offset: f32,
    #[output("velocity", "velocity of the current groove step; pass to a sequencer's `velocity`", type = cv, range = (0.0, 5.0))]
    velocity: f32,
    #[output("trigger", "fires at the start of each grooved step", type = trig)]
    trigger: f32,
}

const GROOVE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Timing and velocity groove",
    description: "Pushes steps of the grid early or late and gives each a velocity, for swing \
                  and shuffle on a single sequencer. Every sequencer follows the global groove \
                  from `$setGroove` by default; pass this module and its `velocity` to a \
                  sequencer's `groove` and `velocity` to give it a feel of its own. Notes keep their written lengths.",
    categories: &[ModuleCategory::Sequencer],
    sections: &[DocSection {
        title: "Templates",
        body: "The template is an object; only `timing` is required.",
        table: Some(DocTable {
            columns: &["Field", "What it does"],
            rows: &[
                &[
                    "`timing`",
                    "how late (positive) or early each step lands, in grid steps; the pattern repeats every `timing.length` steps",
                ],
                &[
                    "`velocity`",
                    "each step's velocity from 0 to 1, one per `timing` entry; all full when left out",
                ],
                &[
                    "`grid`",
                    "steps per cycle: 16 (the default) is sixteenths in a 4/4 bar, 8 eighths",
                ],
                &[
                    "`amount`",
                    "how strongly the timing applies: 0 straight, 1 (the default) as written",
                ],
                &[
                    "`velocityAmount`",
                    "how strongly the velocities apply: 0 every step full, 1 (the default) as written",
                ],
                &[
                    "`random`",
                    "the largest random push early or late on each step, in grid steps",
                ],
                &[
                    "`seed`",
                    "picks the random pushes, so a groove keeps the same feel each time",
                ],
            ],
        }),
        examples: &[],
    }],
    examples: &[DocExample {
        title: "Shuffled eighths with soft offbeats",
        code: r#"
            const g = $groove({
              timing: [0, 0.3],
              velocity: [1, 0.6],
              grid: 8,
            })
            const seq = $cycle($p('c4 e4 g4 c5 g4 e4 c4 g3'), {
              groove: g,
              velocity: g.velocity,
            })
            $sine(seq).amp($adsr(seq.gate)).amp(seq.velocity).out()
        "#,
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
