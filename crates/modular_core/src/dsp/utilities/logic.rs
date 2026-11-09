use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::utils::{TempGate, TempGateState, min_gate_samples},
    poly::{PolyOutput, PolySignal},
};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct RisingEdgeDetectorParams {
    #[input("signal watched for any rise from one sample to the next", type = audio)]
    input: PolySignal,
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
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
        "high while the input keeps rising, and for a trigger's length after", type = gate,
        default,
        range = (0.0, 5.0)
    )]
    output: PolyOutput,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct FallingEdgeDetectorOutputs {
    #[output(
        "output",
        "high while the input keeps falling, and for a trigger's length after", type = gate,
        default,
        range = (0.0, 5.0)
    )]
    output: PolyOutput,
}

#[derive(Clone, Copy)]
struct EdgeChannelState {
    last_input: f32,
    trigger_gate: TempGate,
}

impl Default for EdgeChannelState {
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
            const notes = $cycle($p('c3 e3 g3 e3 c4 g3'))
            $sine(notes)
              .amp($perc($rising(notes), { decay: 0.3 }))
              .out()
        "#,
    }],
};

#[module(name = "$rising", docs = RISING_DOCS, args(input))]
pub struct RisingEdgeDetector {
    outputs: RisingEdgeDetectorOutputs,
    params: RisingEdgeDetectorParams,
    channel_state: Box<[EdgeChannelState]>,
}

impl RisingEdgeDetector {
    pub fn update(&mut self, sample_rate: f32) {
        let num_channels = self.channel_count();
        let hold = min_gate_samples(sample_rate);

        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];
            let input = self.params.input.get_value(ch);

            if input > state.last_input {
                state
                    .trigger_gate
                    .set_state(TempGateState::High, TempGateState::Low, hold);
            }

            state.last_input = input;
            self.outputs.output.set(ch, state.trigger_gate.process());
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
            const notes = $cycle($p('c3 e3 g3 e3 c4 g3'))
            $sine(notes)
              .amp($perc($falling(notes), { decay: 0.3 }))
              .out()
        "#,
    }],
};

#[module(name = "$falling", docs = FALLING_DOCS, args(input))]
pub struct FallingEdgeDetector {
    outputs: FallingEdgeDetectorOutputs,
    params: FallingEdgeDetectorParams,
    channel_state: Box<[EdgeChannelState]>,
}

impl FallingEdgeDetector {
    pub fn update(&mut self, sample_rate: f32) {
        let num_channels = self.channel_count();
        let hold = min_gate_samples(sample_rate);

        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];
            let input = self.params.input.get_value(ch);

            if input < state.last_input {
                state
                    .trigger_gate
                    .set_state(TempGateState::High, TempGateState::Low, hold);
            }

            state.last_input = input;
            self.outputs.output.set(ch, state.trigger_gate.process());
        }
    }
}

message_handlers!(impl FallingEdgeDetector {});
