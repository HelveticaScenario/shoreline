use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::utils::{TempGate, TempGateState, min_gate_samples},
    poly::{PolyOutput, PolySignal},
};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct RisingEdgeDetectorParams {
    #[input("signal watched for any rise from one sample to the next", type = audio)]
    input: PolySignal,
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct FallingEdgeDetectorParams {
    #[input("signal watched for any fall from one sample to the next", type = audio)]
    input: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct RisingEdgeDetectorOutputs {
    #[output(
        "output",
        "high while the input keeps rising, and for a trigger's length after",
        type = gate,
        default
    )]
    output: PolyOutput,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct FallingEdgeDetectorOutputs {
    #[output(
        "output",
        "high while the input keeps falling, and for a trigger's length after",
        type = gate,
        default
    )]
    output: PolyOutput,
}

#[derive(Clone, Copy)]
struct EdgeLaneState {
    last_input: f32,
    trigger_gate: TempGate,
}

impl Default for EdgeLaneState {
    fn default() -> Self {
        Self {
            last_input: 0.0,
            trigger_gate: TempGate::new_gate(TempGateState::Low),
        }
    }
}

const RISING_DOCS: ModuleDocs = ModuleDocs {
    summary: "Trigger on a rising signal",
    description: "Goes high whenever its input rises, so ramps, envelopes, LFOs and \
                  melodies can trigger events. Each upward step of a stepped signal \
                  gives one trigger.",
    categories: &[ModuleCategory::Gate],
    sections: &[],
    examples: &[DocExample {
        title: "Only the upward steps sound",
        code: r#"
            const notes = $`c3 e3 g3 e3 c4 g3`
            $sine(notes)
              .amp($perc($rising(notes), { decay: 0.3 }))
              .out()
        "#,
    }],
};

#[module(name = "$rising", stereo = mono, docs = RISING_DOCS, args(input))]
pub struct RisingEdgeDetector {
    outputs: RisingEdgeDetectorOutputs,
    params: RisingEdgeDetectorParams,
    lane_state: Box<[EdgeLaneState]>,
}

impl RisingEdgeDetector {
    pub fn update(&mut self, sample_rate: f32) {
        let num_lanes = self.lane_count();
        let hold = min_gate_samples(sample_rate);

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];
            let input = self.params.input.get_value(lane);

            if input > state.last_input {
                state
                    .trigger_gate
                    .set_state(TempGateState::High, TempGateState::Low, hold);
            }

            state.last_input = input;
            self.outputs.output.set(lane, state.trigger_gate.process());
        }
    }
}

message_handlers!(impl RisingEdgeDetector {});

const FALLING_DOCS: ModuleDocs = ModuleDocs {
    summary: "Trigger on a falling signal",
    description: "Goes high whenever its input falls, so the downward side of a ramp, \
                  LFO or melody can trigger events. Each downward step of a stepped \
                  signal gives one trigger.",
    categories: &[ModuleCategory::Gate],
    sections: &[],
    examples: &[DocExample {
        title: "Only the downward steps sound",
        code: r#"
            const notes = $`c3 e3 g3 e3 c4 g3`
            $sine(notes)
              .amp($perc($falling(notes), { decay: 0.3 }))
              .out()
        "#,
    }],
};

#[module(name = "$falling", stereo = mono, docs = FALLING_DOCS, args(input))]
pub struct FallingEdgeDetector {
    outputs: FallingEdgeDetectorOutputs,
    params: FallingEdgeDetectorParams,
    lane_state: Box<[EdgeLaneState]>,
}

impl FallingEdgeDetector {
    pub fn update(&mut self, sample_rate: f32) {
        let num_lanes = self.lane_count();
        let hold = min_gate_samples(sample_rate);

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];
            let input = self.params.input.get_value(lane);

            if input < state.last_input {
                state
                    .trigger_gate
                    .set_state(TempGateState::High, TempGateState::Low, hold);
            }

            state.last_input = input;
            self.outputs.output.set(lane, state.trigger_gate.process());
        }
    }
}

message_handlers!(impl FallingEdgeDetector {});
