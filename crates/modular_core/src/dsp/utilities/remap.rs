use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input;
use crate::dsp::utils::map_range;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct RemapParams {
    #[input("signal to rescale", type = audio)]
    input: PolySignal,
    #[input("output value for an input at inMin", type = control, default = -5.0, control_range = (-5.0, 5.0))]
    out_min: PolySignal,
    #[input("output value for an input at inMax", type = control, default = 5.0, control_range = (-5.0, 5.0))]
    out_max: PolySignal,
    #[input("input value that maps to outMin", type = control, default = -5.0, control_range = (-5.0, 5.0))]
    in_min: PolySignal,
    #[input("input value that maps to outMax", type = control, default = 5.0, control_range = (-5.0, 5.0))]
    in_max: PolySignal,
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
    #[output("output", "the rescaled signal", type = cv, default)]
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
        code: r#"
            const duck = $perc($clock._beatTrigger, { decay: 0.3 })
            $tr808.kick($clock._beatTrigger).out()
            $saw($chord('C3[m7]'))
              .amp($remap(duck, 5, 0, 0, 5))
              .out()
        "#,
    }],
};

#[module(
    name = "$remap",
    docs = REMAP_DOCS,
    args(input, outMin, outMax, inMin, inMax),
    block
)]
pub struct Remap {
    outputs: RemapOutputs,
    channel_state: Box<[ChannelState]>,
    params: RemapParams,
}

impl Remap {
    /// Outside a glide, reads each range param raw, exactly as `process_block`
    /// does.
    fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        for i in 0..self.channel_count() {
            let state = &mut self.channel_state[i];
            let in_min =
                state
                    .in_min
                    .follow(p.in_min.get_value(i), p.in_min.glide_mark(i), sample_rate);
            let in_max =
                state
                    .in_max
                    .follow(p.in_max.get_value(i), p.in_max.glide_mark(i), sample_rate);
            let out_min =
                state
                    .out_min
                    .follow(p.out_min.get_value(i), p.out_min.glide_mark(i), sample_rate);
            let out_max =
                state
                    .out_max
                    .follow(p.out_max.get_value(i), p.out_max.glide_mark(i), sample_rate);
            let value = map_range(p.input.get_value(i), in_min, in_max, out_min, out_max);
            self.outputs.sample.set(i, value);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut RemapBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let p = &self.params;
        let gliding = self.channel_state[..channels]
            .iter()
            .enumerate()
            .any(|(i, cs)| {
                cs.in_min.will_glide(p.in_min.glide_mark(i))
                    || cs.in_max.will_glide(p.in_max.glide_mark(i))
                    || cs.out_min.will_glide(p.out_min.glide_mark(i))
                    || cs.out_max.will_glide(p.out_max.glide_mark(i))
            });
        if gliding {
            return None;
        }
        let mut scratch = Default::default();
        let inputs = p.block_inputs(range.clone(), channels, &mut scratch)?;
        let last = range.end - 1;
        block_input::map(
            &mut outputs.sample,
            range,
            [
                inputs.input,
                inputs.in_min,
                inputs.in_max,
                inputs.out_min,
                inputs.out_max,
            ],
            |[x, in_min, in_max, out_min, out_max]| map_range(x, in_min, in_max, out_min, out_max),
        );
        for (i, cs) in self.channel_state[..channels].iter_mut().enumerate() {
            cs.in_min
                .settle(inputs.in_min.row(last)[i], p.in_min.glide_mark(i));
            cs.in_max
                .settle(inputs.in_max.row(last)[i], p.in_max.glide_mark(i));
            cs.out_min
                .settle(inputs.out_min.row(last)[i], p.out_min.glide_mark(i));
            cs.out_max
                .settle(inputs.out_max.row(last)[i], p.out_max.glide_mark(i));
        }
        Some(())
    }
}

message_handlers!(impl Remap {});
