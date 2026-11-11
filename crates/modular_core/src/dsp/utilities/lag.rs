use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::utils::sanitize,
    poly::{PolyOutput, PolySignal, PolySignalExt},
};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct LagProcessorParams {
    #[input("signal to smooth", type = audio)]
    input: PolySignal,
    #[input(
        "seconds the output takes to climb 1 V; follows fall when not patched",
        type = control,
        range = (0.001, ..),
        control_range = (0.001, 10.0),
    )]
    #[deserr(default)]
    rise: Option<PolySignal>,
    #[input(
        "seconds the output takes to drop 1 V",
        type = control,
        default = 0.01,
        range = (0.001, ..),
        control_range = (0.001, 10.0),
    )]
    #[deserr(default)]
    fall: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct LagProcessorOutputs {
    #[output("output", "the smoothed signal", type = cv, default)]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct SlewChannelState {
    current_value: f32,
    initialized: bool,
}

const SLEW_DOCS: ModuleDocs = ModuleDocs {
    summary: "Slew limiter",
    description: "Limits how fast a signal can change, so jumps become glides: \
                  portamento on a pitch, softened steps on stepped modulation, or a \
                  gate rounded into a rise and fall. Rising and falling can glide at \
                  different rates.",
    categories: &[ModuleCategory::Utility],
    sections: &[],
    examples: &[
        DocExample {
            title: "Gliding bassline",
            code: r#"
                $saw($slew($p('c2 c3 eb2 g2').c, { fall: 0.1 }))
                  .$.lpf('c5', 2)
                  .out()
            "#,
        },
        DocExample {
            title: "Random notes that slide",
            code: r#"
                $sine(
                  $slew(
                    $sah(
                      $noise('white').range('c3', 'c5'),
                      $clock.beatTrigger,
                    ),
                    { fall: 0.05 },
                  ),
                ).out()
            "#,
        },
    ],
};

#[module(name = "$slew", docs = SLEW_DOCS, args(input))]
pub struct LagProcessor {
    outputs: LagProcessorOutputs,
    params: LagProcessorParams,
    channel_state: Box<[SlewChannelState]>,
}

impl LagProcessor {
    pub fn update(&mut self, sample_rate: f32) {
        let num_channels = self.channel_count();

        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];
            let input = self.params.input.get_value(ch);
            if !state.initialized {
                state.current_value = sanitize(input);
                state.initialized = true;
            }

            let fall_time = self.params.fall.value_or(ch, 0.01).max(0.001);
            let rise_time = self.params.rise.value_or(ch, fall_time).max(0.001);

            // Calculate max change per sample
            // time is seconds for 1.0v change (full scale)
            // Slew rate = 1.0 / time (V/s)
            // Max delta per sample = Slew rate / sample_rate
            let max_rise = 1.0 / (rise_time * sample_rate);
            let max_fall = 1.0 / (fall_time * sample_rate);

            let diff = input - state.current_value;

            let change = if diff > 0.0 {
                diff.min(max_rise)
            } else {
                diff.max(-max_fall)
            };

            state.current_value += change;
            state.current_value = sanitize(state.current_value);
            self.outputs.sample.set(ch, state.current_value);
        }
    }
}

message_handlers!(impl LagProcessor {});
