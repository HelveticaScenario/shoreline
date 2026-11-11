use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ClampParams {
    #[input("signal to clamp", type = audio)]
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
    #[output("output", "the clamped signal", type = cv, default)]
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
                  .$.lpf(
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
        let p = &self.params;
        for i in 0..self.channel_count() {
            let min = p.min.as_ref().map(|min| min.get_value(i));
            let max = p.max.as_ref().map(|max| max.get_value(i));
            self.outputs
                .sample
                .set(i, clamp(p.input.get_value(i), min, max));
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut ClampBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let mut scratch = Default::default();
        let inputs = self
            .params
            .block_inputs(range.clone(), self.channel_count(), &mut scratch)?;
        let (input, out) = (inputs.input, &mut outputs.sample);
        match (inputs.min, inputs.max) {
            (Some(min), Some(max)) => {
                block_input::map(out, range, [input, min, max], |[x, min, max]| {
                    clamp(x, Some(min), Some(max))
                });
            }
            (Some(min), None) => {
                block_input::map(out, range, [input, min], |[x, min]| {
                    clamp(x, Some(min), None)
                });
            }
            (None, Some(max)) => {
                block_input::map(out, range, [input, max], |[x, max]| {
                    clamp(x, None, Some(max))
                });
            }
            (None, None) => block_input::map(out, range, [input], |[x]| x),
        }
        Some(())
    }
}

message_handlers!(impl Clamp {});
