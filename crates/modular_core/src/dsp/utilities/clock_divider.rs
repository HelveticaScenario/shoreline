use deserr::Deserr;
use napi::Result;
use schemars::JsonSchema;

use crate::dsp::utils::{SchmittTrigger, TempGate, TempGateState, min_gate_samples};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::types::ClockMessages;

fn default_division() -> u32 {
    1
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ClockDividerParams {
    #[input("clock to divide", type = trig)]
    pub input: PolySignal,
    #[input("how many input ticks per output tick: 2 fires every other tick", range = (1, ..))]
    pub division: u32,
    #[input("rising edge restarts the count, so the next tick fires", type = trig)]
    #[deserr(default)]
    pub reset: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ClockDividerOutputs {
    #[output("output", "a short trigger every `division` ticks", type = trig, default, range = (0.0, 5.0))]
    pub output: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct ChannelState {
    counter: u32,
    input_schmitt: SchmittTrigger,
    reset_schmitt: SchmittTrigger,
    trigger_gate: TempGate,
}

const CLOCK_DIVIDER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Clock divider",
    description: "Fires once every so many input ticks, turning one clock into slower \
                  ones for half-time parts, longer phrases and polyrhythms.",
    categories: &[ModuleCategory::Gate, ModuleCategory::Sequencer],
    sections: &[],
    examples: &[DocExample {
        title: "Bass on every other beat",
        code: r#"
            $saw('c2')
              .amp(
                $perc($clockDivider($clock.beatTrigger, 2), {
                  decay: 0.3,
                }),
              )
              .out()
            $noise('white')
              .amp($perc($clock.beatTrigger, { decay: 0.03 }))
              .out()
        "#,
    }],
};

#[module(
    name = "$clockDivider",
    docs = CLOCK_DIVIDER_DOCS,
    args(input, division)
)]
pub struct ClockDivider {
    params: ClockDividerParams,
    outputs: ClockDividerOutputs,
    channel_state: Box<[ChannelState]>,
}

message_handlers!(impl ClockDivider {
    Clock(m) => ClockDivider::on_clock_message,
});

impl ClockDivider {
    fn on_clock_message(&mut self, m: &ClockMessages) -> Result<()> {
        match m {
            ClockMessages::Start => {
                // Reset all channel counters on start
                for state in self.channel_state.iter_mut() {
                    state.counter = 0;
                }
            }
            ClockMessages::Stop => {
                // No special handling needed on stop
            }
        }
        Ok(())
    }

    fn update(&mut self, sample_rate: f32) {
        let num_channels = self.channel_count();
        let division = self.params.division.max(1);
        let hold = min_gate_samples(sample_rate);

        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];

            // Reset counter on rising edge of reset trigger
            if state
                .reset_schmitt
                .process(self.params.reset.value_or_zero(ch))
            {
                state.counter = 0;
            }

            if state.input_schmitt.process(self.params.input.get_value(ch)) {
                if state.counter == 0 {
                    state
                        .trigger_gate
                        .set_state(TempGateState::High, TempGateState::Low, hold);
                }

                state.counter += 1;
                if state.counter >= division {
                    state.counter = 0;
                }
            }

            self.outputs.output.set(ch, state.trigger_gate.process());
        }
    }
}
