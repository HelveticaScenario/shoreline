use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
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
    min: Option<PolySignal>,
    #[input(
        "highest value let through; leave out to keep the top unclamped",
        type = control,
        control_range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    max: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ClampOutputs {
    #[output("output", "the clamped signal", default)]
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
            code: "$saw('c2').$.lpf($clamp($sine('0.2hz').range('c3', 'c8'), { max: 'c6' }), 2).out()",
        },
    ],
};

#[module(name = "$clamp", docs = CLAMP_DOCS, args(input))]
pub struct Clamp {
    outputs: ClampOutputs,
    params: ClampParams,
}

impl Clamp {
    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let has_min = !self.params.min.is_disconnected();
        let has_max = !self.params.max.is_disconnected();

        for i in 0..channels as usize {
            let mut val = self.params.input.get_value(i);

            match (has_min, has_max) {
                (true, true) => {
                    let a = self.params.min.value_or_zero(i);
                    let b = self.params.max.value_or_zero(i);
                    let (lo, hi) = if b < a { (b, a) } else { (a, b) };
                    val = val.clamp(lo, hi);
                }
                (true, false) => {
                    let min_val = self.params.min.value_or_zero(i);
                    if val < min_val {
                        val = min_val;
                    }
                }
                (false, true) => {
                    let max_val = self.params.max.value_or_zero(i);
                    if val > max_val {
                        val = max_val;
                    }
                }
                (false, false) => {}
            }

            self.outputs.sample.set(i, val);
        }
    }
}

message_handlers!(impl Clamp {});
