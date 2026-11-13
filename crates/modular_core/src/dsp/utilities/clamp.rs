use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input::{self, InputScratch};
use crate::bounds::Bounds;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};

#[params(declares_range = (min, max))]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ClampParams {
    #[input("signal to clamp", type = audio, bounds)]
    input: PolySignal,
    #[input(
        "lowest value let through; leave out to keep the bottom unclamped",
        type = control,
        control_range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    #[block_input(optional)]
    min: Option<PolySignal>,
    #[input(
        "highest value let through; leave out to keep the top unclamped",
        type = control,
        control_range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    #[block_input(optional)]
    max: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ClampOutputs {
    #[output("output", "the clamped signal", type = cv, bounds = input, default)]
    sample: PolyOutput,
}

const CLAMP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Limit a signal to a range",
    description: "Holds a signal between a floor and a ceiling, flattening anything \
                  past either edge. Give only one bound to clamp one side: handy for \
                  keeping modulation from pushing a cutoff or pitch somewhere \
                  unpleasant.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[
        DocExample {
            title: "Half-wave rectified buzz",
            code: "$clamp($sine('c3'), { min: 0 }).out()",
        },
        DocExample {
            title: "Sweep that lingers wide open",
            code: r#"
                $saw('c2')
                  .lpf(
                    $clamp($sine('0.2hz').range('c3', 'c8'), { max: 'c6' }),
                    2,
                  )
                  .out()
            "#,
        },
    ],
};

#[module(name = "$clamp", docs = CLAMP_DOCS, args(input), block)]
pub struct Clamp {
    outputs: ClampOutputs,
    params: ClampParams,
}

/// `x` held within the connected bounds; two bounds in either order. A NaN
/// bound lets `x` through on that side rather than panicking as `f32::clamp`
/// would.
#[inline(always)]
fn clamp(x: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    match (min, max) {
        (Some(a), Some(b)) => {
            let (lo, hi) = if b < a { (b, a) } else { (a, b) };
            x.max(lo).min(hi)
        }
        (Some(min), None) if x < min => min,
        (None, Some(max)) if x > max => max,
        _ => x,
    }
}

impl Clamp {
    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        let out = &mut self.outputs.sample;
        for i in 0..lanes {
            let min = p.min.as_ref().map(|min| min.get_value(i));
            let max = p.max.as_ref().map(|max| max.get_value(i));
            out.set(i, clamp(p.input.get_value(i), min, max));
            if out.stores_bounds() {
                out.set_bounds(i, p.input.get_bounds(i).map(|x| clamp(x, min, max)));
            }
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut ClampBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let mut scratch = Default::default();
        let inputs = self
            .params
            .block_inputs(range.clone(), lanes, &mut scratch)?;
        let (signal, input, out) = (&self.params.input, inputs.input, &mut outputs.sample);
        // The input's bounds, read once for the range as rows of their ends,
        // then clamped with the limits each slot reads.
        let stores = out.has_bounds();
        if stores && (0..lanes).any(|lane| signal.block_bounds(range.clone(), lane).is_none()) {
            return None;
        }
        let input_bounds = |lane: usize| match stores {
            true => signal
                .block_bounds(range.clone(), lane)
                .unwrap_or(Bounds::UNBOUNDED),
            false => Bounds::UNBOUNDED,
        };
        let (mut s_start, mut s_end) = (InputScratch::new(), InputScratch::new());
        let start = block_input::constant_row(range.clone(), lanes, &mut s_start, |lane| {
            input_bounds(lane).start
        });
        let end = block_input::constant_row(range.clone(), lanes, &mut s_end, |lane| {
            input_bounds(lane).end
        });
        let bounds = |start: f32, end: f32, min: Option<f32>, max: Option<f32>| {
            Some(Bounds::new(start, end).map(|x| clamp(x, min, max)))
        };
        match (inputs.min, inputs.max) {
            (Some(min), Some(max)) => {
                block_input::map_bounds(
                    out,
                    range.clone(),
                    [start, end, min, max],
                    |_, [s, e, min, max]| bounds(s, e, Some(min), Some(max)),
                )?;
                block_input::map(out, range, [input, min, max], |[x, min, max]| {
                    clamp(x, Some(min), Some(max))
                });
            }
            (Some(min), None) => {
                block_input::map_bounds(
                    out,
                    range.clone(),
                    [start, end, min],
                    |_, [s, e, min]| bounds(s, e, Some(min), None),
                )?;
                block_input::map(out, range, [input, min], |[x, min]| {
                    clamp(x, Some(min), None)
                });
            }
            (None, Some(max)) => {
                block_input::map_bounds(
                    out,
                    range.clone(),
                    [start, end, max],
                    |_, [s, e, max]| bounds(s, e, None, Some(max)),
                )?;
                block_input::map(out, range, [input, max], |[x, max]| {
                    clamp(x, None, Some(max))
                });
            }
            (None, None) => {
                block_input::map_bounds(out, range.clone(), [start, end], |_, [s, e]| {
                    bounds(s, e, None, None)
                })?;
                block_input::map(out, range, [input], |[x]| x);
            }
        }
        Some(())
    }
}

message_handlers!(impl Clamp {});
