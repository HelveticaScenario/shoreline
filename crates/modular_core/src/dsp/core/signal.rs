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
#[derive(Clone, Deserr, JsonSchema, ChannelCount, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields, validate = validate_signal_params -> ModuleParamErrors)]
struct SignalParams {
    #[input("the signal or value to pass on", type = audio, bounds)]
    value: PolySignal,
    #[input("bottom of the range the signal declares, whatever its value; set with max")]
    #[serde(default)]
    #[deserr(default)]
    min: Option<f32>,
    #[input("top of the range the signal declares, whatever its value; set with min")]
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
    channel_state: Box<[SwapGlide]>,
}

impl Signal {
    fn update(&mut self, sample_rate: f32) {
        let value = &self.params.value;
        let channels = self.channel_count();
        let out = &mut self.outputs.sample;
        for i in 0..channels {
            let input = value.get_value(i);
            let v = self.channel_state[i].follow(input, value.glide_mark(i), sample_rate);
            out.set(i, v);
            if out.stores_bounds() {
                // Mid-glide the output is the input offset by what remains
                // of the glide; the bounds carry the same offset.
                let offset = v - input;
                let bounds = self
                    .params
                    .declared()
                    .unwrap_or_else(|| value.get_bounds(i).map(|x| x + offset));
                out.set_bounds(i, bounds);
            }
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut SignalBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let glides = &mut self.channel_state[..channels];
        let value = &self.params.value;
        if value.glides_pending(glides) {
            return None;
        }
        let mut scratch = InputScratch::new();
        let input = value.block_input(range.clone(), channels, &mut scratch)?;
        let declared = self.params.declared();
        outputs.sample.fill_bounds_with(range.clone(), |ch| {
            declared.or_else(|| value.block_bounds(range.clone(), ch))
        })?;
        value.settle_glides(glides, input.row(range.end - 1));
        block_input::map(&mut outputs.sample, range, [input], |[x]| x);
        Some(())
    }
}

message_handlers!(impl Signal {});
