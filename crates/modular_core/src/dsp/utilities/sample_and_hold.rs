use crate::dsp::utils::{SchmittState, SchmittTrigger};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SampleAndHoldParams {
    #[input("signal to sample", type = audio)]
    input: PolySignal,
    #[input("rising edge captures the current input value", type = trig)]
    trigger: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SampleAndHoldOutputs {
    #[output("output", "the voltage captured at the last trigger", default)]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct SampleAndHoldChannelState {
    trigger: SchmittTrigger,
    held_value: f32,
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
                      $clock.beatTrigger,
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
                  .$.lpf(
                    $sah(
                      $noise('white').range('c4', 'c7'),
                      $clock.beatTrigger,
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
    channel_state: Box<[SampleAndHoldChannelState]>,
}

impl SampleAndHold {
    pub fn update(&mut self, _sample_rate: f32) {
        let num_channels = self.channel_count();

        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];
            let input = self.params.input.get_value(ch);
            let trigger = self.params.trigger.get_value(ch);

            if state.trigger.state == SchmittState::Uninitialized {
                state.held_value = input;
            }

            // Make sure to initialize the held value on the first update
            if state.trigger.process(trigger) {
                state.held_value = input;
            }

            self.outputs.sample.set(ch, state.held_value);
        }
    }
}

message_handlers!(impl SampleAndHold {});

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct TrackAndHoldParams {
    #[input("signal to track", type = audio)]
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
    #[output("output", "the tracked or held voltage", default)]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct TrackAndHoldChannelState {
    gate: SchmittTrigger,
    /// The output voltage, kept here rather than read back from the output port
    /// so it survives the patch update that reconstructs the module's outputs.
    held_value: f32,
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
    channel_state: Box<[TrackAndHoldChannelState]>,
}

impl TrackAndHold {
    pub fn update(&mut self, _sample_rate: f32) {
        let num_channels = self.channel_count();

        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];
            let input = self.params.input.get_value(ch);
            let gate = self.params.gate.get_value(ch);

            state.gate.process(gate);
            if state.gate.state() != SchmittState::High {
                state.held_value = input;
            }

            self.outputs.sample.set(ch, state.held_value);
        }
    }
}

message_handlers!(impl TrackAndHold {});
