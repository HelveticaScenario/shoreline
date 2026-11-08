use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct RemapParams {
    #[input("signal to rescale", type = audio)]
    input: PolySignal,
    #[input("input value that maps to outMin", type = control, default = -5.0, control_range = (-5.0, 5.0))]
    in_min: PolySignal,
    #[input("input value that maps to outMax", type = control, default = 5.0, control_range = (-5.0, 5.0))]
    in_max: PolySignal,
    #[input("output value for an input at inMin", type = control, default = -5.0, control_range = (-5.0, 5.0))]
    out_min: PolySignal,
    #[input("output value for an input at inMax", type = control, default = 5.0, control_range = (-5.0, 5.0))]
    out_max: PolySignal,
}

#[derive(Default, Clone, Copy)]
struct ChannelState {
    in_min: SwapGlide,
    in_max: SwapGlide,
    out_min: SwapGlide,
    out_max: SwapGlide,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct RemapOutputs {
    #[output("output", "the rescaled signal", default)]
    sample: PolyOutput,
}

const REMAP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Rescale between ranges",
    description: "Maps a signal from one voltage range onto another, stretching, \
                  shifting or flipping it on the way. It is what `.range()` uses; \
                  reach for it directly to flip a signal or to stretch just part of \
                  its swing over the target.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Pad ducked by an upside-down envelope",
        code: "const duck = $perc($clock.beatTrigger, { decay: 0.3 })\n\
               $tr808.kick($clock.beatTrigger).out()\n\
               $saw($chord('C3[m7]')).amp($remap(duck, 5, 0, 0, 5)).out()",
    }],
};

#[module(
    name = "$remap",
    docs = REMAP_DOCS,
    args(input, outMin, outMax, inMin, inMax)
)]
pub struct Remap {
    outputs: RemapOutputs,
    channel_state: Box<[ChannelState]>,
    params: RemapParams,
}

impl Remap {
    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();

        for i in 0..channels as usize {
            let input_val = self.params.input.get_value(i);
            let state = &mut self.channel_state[i];

            state.in_min.update(
                self.params.in_min.get_value(i),
                self.params.in_min.glide_mark(i),
                sample_rate,
            );
            state.in_max.update(
                self.params.in_max.get_value(i),
                self.params.in_max.glide_mark(i),
                sample_rate,
            );
            state.out_min.update(
                self.params.out_min.get_value(i),
                self.params.out_min.glide_mark(i),
                sample_rate,
            );
            state.out_max.update(
                self.params.out_max.get_value(i),
                self.params.out_max.glide_mark(i),
                sample_rate,
            );

            // Apply remapping using map_range utility
            let output = crate::dsp::utils::map_range(
                input_val,
                *state.in_min,
                *state.in_max,
                *state.out_min,
                *state.out_max,
            );

            self.outputs.sample.set(i, output);
        }
    }
}

message_handlers!(impl Remap {});
