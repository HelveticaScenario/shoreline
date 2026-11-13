use crate::bounds::Bounds;
use crate::dsp::utils::{SchmittState, SchmittTrigger};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SampleAndHoldParams {
    #[input("signal to sample", type = audio, bounds)]
    input: PolySignal,
    #[input("rising edge captures the current input value", type = trig)]
    trigger: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SampleAndHoldOutputs {
    #[output("output", "the voltage captured at the last trigger", type = cv, bounds = input, default)]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct SampleAndHoldLaneState {
    trigger: SchmittTrigger,
    held_value: f32,
    /// The input's bounds when `held_value` was captured.
    held_bounds: Bounds,
}

const SAH_DOCS: ModuleDocs = ModuleDocs {
    summary: "Sample and hold",
    description: "Grabs its input's value on each trigger and holds it until the next: \
                  the classic way to turn noise into stepped random melodies and \
                  modulation.",
    categories: &[ModuleCategory::Gate],
    sections: &[],
    examples: &[
        DocExample {
            title: "Random notes in C major",
            code: r#"
                $sine(
                  $quantizer(
                    $sah(
                      $noise('white').range('c3', 'c5'),
                      $clock._beatTrigger,
                    ),
                    'C(major)',
                  ),
                ).out()
            "#,
        },
        DocExample {
            title: "Stepped random filter",
            code: r#"
                $saw('c2')
                  .lpf(
                    $sah(
                      $noise('white').range('c4', 'c7'),
                      $clock._beatTrigger,
                    ),
                    3,
                  )
                  .out()
            "#,
        },
    ],
};

#[module(name = "$sah", docs = SAH_DOCS, args(input, trigger))]
pub struct SampleAndHold {
    outputs: SampleAndHoldOutputs,
    params: SampleAndHoldParams,
    lane_state: Box<[SampleAndHoldLaneState]>,
}

impl SampleAndHold {
    pub fn update(&mut self, _sample_rate: f32) {
        let num_lanes = self.lane_count();

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];
            let input = self.params.input.get_value(lane);
            let trigger = self.params.trigger.get_value(lane);

            let uninitialized = state.trigger.state == SchmittState::Uninitialized;
            if state.trigger.process(trigger) || uninitialized {
                state.held_value = input;
                state.held_bounds = self.params.input.get_bounds(lane);
            }

            self.outputs.sample.set(lane, state.held_value);
            // A hold taken before this output stored bounds has none; it
            // reads its input's until it next takes one.
            let bounds = if state.held_bounds.is_bounded() {
                state.held_bounds
            } else {
                self.params.input.get_bounds(lane)
            };
            self.outputs.sample.set_bounds(lane, bounds);
        }
    }
}

message_handlers!(impl SampleAndHold {});

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct TrackAndHoldParams {
    #[input("signal to track", type = audio, bounds)]
    input: PolySignal,
    #[input(
        "output follows the input while low and holds its last value while high",
        type = gate,
    )]
    gate: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TrackAndHoldOutputs {
    #[output("output", "the tracked or held voltage", type = cv, bounds = input, default)]
    sample: PolyOutput,
}

#[derive(Clone, Copy)]
struct TrackAndHoldLaneState {
    gate: SchmittTrigger,
    /// The output voltage, kept here rather than read back from the output port
    /// so it survives the patch update that reconstructs the module's outputs.
    held_value: f32,
    /// The input's bounds when `held_value` was taken.
    held_bounds: Bounds,
}

/// A gate high from the start holds 0 V until it first falls.
impl Default for TrackAndHoldLaneState {
    fn default() -> Self {
        Self {
            gate: SchmittTrigger::default(),
            held_value: 0.0,
            held_bounds: Bounds::point(0.0),
        }
    }
}

const TAH_DOCS: ModuleDocs = ModuleDocs {
    summary: "Track and hold",
    description: "Follows its input while the gate is low and freezes it while the \
                  gate is high: the counterpart of `$sah`, for catching a moving value \
                  and holding it.",
    categories: &[ModuleCategory::Gate],
    sections: &[],
    examples: &[DocExample {
        title: "Glide that freezes in steps",
        code: r#"
            $sine(
              $tah($sine('0.3hz').range('c3', 'c5'), $pulse('1hz')),
            ).out()
        "#,
    }],
};

#[module(name = "$tah", docs = TAH_DOCS, args(input, gate))]
pub struct TrackAndHold {
    outputs: TrackAndHoldOutputs,
    params: TrackAndHoldParams,
    lane_state: Box<[TrackAndHoldLaneState]>,
}

impl TrackAndHold {
    pub fn update(&mut self, _sample_rate: f32) {
        let num_lanes = self.lane_count();

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];
            let input = self.params.input.get_value(lane);
            let gate = self.params.gate.get_value(lane);

            state.gate.process(gate);
            if state.gate.state() != SchmittState::High {
                state.held_value = input;
                state.held_bounds = self.params.input.get_bounds(lane);
            }

            self.outputs.sample.set(lane, state.held_value);
            // A hold taken before this output stored bounds has none; it
            // reads its input's until it next takes one.
            let bounds = if state.held_bounds.is_bounded() {
                state.held_bounds
            } else {
                self.params.input.get_bounds(lane)
            };
            self.outputs.sample.set_bounds(lane, bounds);
        }
    }
}

message_handlers!(impl TrackAndHold {});
