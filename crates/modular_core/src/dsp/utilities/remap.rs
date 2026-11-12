use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input::{self, InputScratch};
use crate::bounds::Bounds;
use crate::dsp::utils::map_range;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use crate::types::SwapGlide;

#[params(reads_bounds = remap_reads_bounds)]
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
    #[input(
        "input value that maps to outMin",
        type = control,
        default = -5.0,
        default_doc = "the start of the input's range when it has one, otherwise -5",
        control_range = (-5.0, 5.0),
    )]
    #[block_input(optional)]
    #[deserr(default)]
    in_min: Option<PolySignal>,
    #[input(
        "input value that maps to outMax",
        type = control,
        default = 5.0,
        default_doc = "the end of the input's range when it has one, otherwise 5",
        control_range = (-5.0, 5.0),
    )]
    #[block_input(optional)]
    #[deserr(default)]
    in_max: Option<PolySignal>,
}

/// An unset input range follows the input's bounds.
fn remap_reads_bounds(p: &RemapParams) -> Vec<&PolySignal> {
    if p.in_min.is_none() || p.in_max.is_none() {
        vec![&p.input]
    } else {
        Vec::new()
    }
}

/// The input range an unset `inMin`/`inMax` reads: the input's bounds in
/// their order, so a signal ranged `(5, 0)` maps back the right way up, or
/// ±5 for an input without a range.
#[inline(always)]
fn input_range(bounds: Bounds) -> (f32, f32) {
    if bounds.swings() {
        (bounds.start, bounds.end)
    } else {
        (RemapParams::IN_MIN_DEFAULT, RemapParams::IN_MAX_DEFAULT)
    }
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
    #[output("output", "the rescaled signal", type = cv, bounds = params, default)]
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
            let unset = if p.in_min.is_none() || p.in_max.is_none() {
                input_range(p.input.get_bounds(i))
            } else {
                (0.0, 0.0)
            };
            let in_min = match &p.in_min {
                Some(in_min) => {
                    state
                        .in_min
                        .follow(in_min.get_value(i), in_min.glide_mark(i), sample_rate)
                }
                None => unset.0,
            };
            let in_max = match &p.in_max {
                Some(in_max) => {
                    state
                        .in_max
                        .follow(in_max.get_value(i), in_max.glide_mark(i), sample_rate)
                }
                None => unset.1,
            };
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
            // The output range as declared: a signal that strays past
            // `inMin`/`inMax` strays past it too.
            self.outputs
                .sample
                .set_bounds(i, Bounds::new(out_min, out_max));
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
        let glides = |signal: &Option<PolySignal>, glide: &SwapGlide, i: usize| {
            signal
                .as_ref()
                .is_some_and(|signal| glide.will_glide(signal.glide_mark(i)))
        };
        let gliding = self.channel_state[..channels]
            .iter()
            .enumerate()
            .any(|(i, cs)| {
                glides(&p.in_min, &cs.in_min, i)
                    || glides(&p.in_max, &cs.in_max, i)
                    || cs.out_min.will_glide(p.out_min.glide_mark(i))
                    || cs.out_max.will_glide(p.out_max.glide_mark(i))
            });
        if gliding {
            return None;
        }
        // An unset input range reads the input's bounds, which hold still
        // over the range or send the slots to `update`.
        if (p.in_min.is_none() || p.in_max.is_none())
            && (0..channels).any(|ch| p.input.block_bounds(range.clone(), ch).is_none())
        {
            return None;
        }
        let unset = |ch: usize| {
            input_range(
                p.input
                    .block_bounds(range.clone(), ch)
                    .unwrap_or(Bounds::UNBOUNDED),
            )
        };
        let mut scratch = Default::default();
        let inputs = p.block_inputs(range.clone(), channels, &mut scratch)?;
        let (mut s_min, mut s_max) = (InputScratch::new(), InputScratch::new());
        let in_min = inputs.in_min.unwrap_or_else(|| {
            block_input::constant_row(range.clone(), channels, &mut s_min, |ch| unset(ch).0)
        });
        let in_max = inputs.in_max.unwrap_or_else(|| {
            block_input::constant_row(range.clone(), channels, &mut s_max, |ch| unset(ch).1)
        });
        let last = range.end - 1;
        block_input::map_bounds(
            &mut outputs.sample,
            range.clone(),
            [inputs.out_min, inputs.out_max],
            |_, [out_min, out_max]| Some(Bounds::new(out_min, out_max)),
        )?;
        block_input::map(
            &mut outputs.sample,
            range,
            [inputs.input, in_min, in_max, inputs.out_min, inputs.out_max],
            |[x, in_min, in_max, out_min, out_max]| map_range(x, in_min, in_max, out_min, out_max),
        );
        for (i, cs) in self.channel_state[..channels].iter_mut().enumerate() {
            if let Some(signal) = &p.in_min {
                cs.in_min.settle(in_min.row(last)[i], signal.glide_mark(i));
            }
            if let Some(signal) = &p.in_max {
                cs.in_max.settle(in_max.row(last)[i], signal.glide_mark(i));
            }
            cs.out_min
                .settle(inputs.out_min.row(last)[i], p.out_min.glide_mark(i));
            cs.out_max
                .settle(inputs.out_max.row(last)[i], p.out_max.glide_mark(i));
        }
        Some(())
    }
}

message_handlers!(impl Remap {});
