use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input::{self, InputScratch};
use crate::bounds::Bounds;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::param_errors::{ModuleParamErrors, check_declared_range};
use crate::poly::{PolyOutput, PolySignal};
use crate::types::SwapGlide;

#[params(declares_range = (min, max))]
#[derive(Clone, Deserr, JsonSchema, LaneCount, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields, validate = validate_signal_params -> ModuleParamErrors)]
struct SignalParams {
    #[input("the signal or value to pass on", type = audio, bounds)]
    value: PolySignal,
    #[input(
        "bottom of the range the signal sweeps when its value has none of its own; set with max"
    )]
    #[serde(default)]
    #[deserr(default)]
    min: Option<f32>,
    #[input("top of the range the signal sweeps when its value has none of its own; set with min")]
    #[serde(default)]
    #[deserr(default)]
    max: Option<f32>,
}

fn validate_signal_params(
    params: SignalParams,
    _location: deserr::ValuePointerRef,
) -> Result<SignalParams, ModuleParamErrors> {
    check_declared_range(params.min, params.max)?;
    Ok(params)
}

impl SignalParams {
    /// The range `min` and `max` declare, when both are set.
    fn declared(&self) -> Option<Bounds> {
        Some(Bounds::new(self.min?, self.max?))
    }
}

/// The output's bounds: the value's range, or the declared one for a value
/// that holds still or has none.
#[inline(always)]
fn output_bounds(value: Bounds, declared: Option<Bounds>) -> Bounds {
    match declared {
        Some(declared) if !value.swings() => declared,
        _ => value,
    }
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SignalOutputs {
    #[output("output", "the input, unchanged", type = cv, bounds = input, default)]
    sample: PolyOutput,
}

const SIGNAL_DOCS: ModuleDocs = ModuleDocs {
    summary: "Pass-through signal",
    description: "Passes its input on unchanged, giving a value or cable a module of its own: \
                  a named point in the patch to tap, scope or reuse. A constant set here glides \
                  to its new value when the patch updates instead of jumping.",
    categories: &[ModuleCategory::Utility],
    sections: &[],
    examples: &[DocExample {
        title: "One pitch shared by two oscillators",
        code: r#"
            const root = $signal('c3')
            $saw(root).out()
            $pulse(root.add(7 / 12)).out()
        "#,
    }],
};

#[module(name = "$signal", docs = SIGNAL_DOCS, args(value), block)]
pub struct Signal {
    outputs: SignalOutputs,
    params: SignalParams,
    lane_state: Box<[SwapGlide]>,
}

impl Signal {
    fn update(&mut self, sample_rate: f32) {
        let value = &self.params.value;
        let lanes = self.lane_count();
        let out = &mut self.outputs.sample;
        for i in 0..lanes {
            let input = value.get_value(i);
            let v = self.lane_state[i].follow(input, value.glide_mark(i), sample_rate);
            out.set(i, v);
            if out.stores_bounds() {
                // Mid-glide the output is the input offset by what remains
                // of the glide; the bounds carry the same offset.
                let offset = v - input;
                let bounds = value.get_bounds(i).map(|x| x + offset);
                out.set_bounds(i, output_bounds(bounds, self.params.declared()));
            }
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut SignalBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let glides = &mut self.lane_state[..lanes];
        let value = &self.params.value;
        if value.glides_pending(glides) {
            return None;
        }
        let mut scratch = InputScratch::new();
        let input = value.block_input(range.clone(), lanes, &mut scratch)?;
        let declared = self.params.declared();
        outputs.sample.fill_bounds_with(range.clone(), |lane| {
            Some(output_bounds(
                value.block_bounds(range.clone(), lane)?,
                declared,
            ))
        })?;
        value.settle_glides(glides, input.row(range.end - 1));
        block_input::map(&mut outputs.sample, range, [input], |[x]| x);
        Some(())
    }
}

message_handlers!(impl Signal {});
