use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{LaneSum, LaneSumExt, PolyOutput};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SpreadParams {
    #[input("value of the first channel", type = control, control_range = (-5.0, 5.0))]
    min: LaneSum,
    #[input("value of the last channel", type = control, control_range = (-5.0, 5.0))]
    max: LaneSum,
    #[input(
        "number of output channels; with 1, a single value between min and max placed by bias",
        range = (1, 64),
    )]
    count: usize,
    #[input(
        "bunches the channels toward max (positive) or min (negative); 0 spaces them evenly",
        type = control,
        default = 0.0,
        control_range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    bias: Option<LaneSum>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SpreadOutputs {
    #[output("output", "one value per channel, from min to max", type = cv, default)]
    sample: PolyOutput,
}

const SPREAD_DOCS: ModuleDocs = ModuleDocs {
    summary: "Evenly spaced channel values",
    description: "Fills a set of channels with values spaced from `min` to `max`, for \
                  fanning pitches, pans or detunes across a polyphonic patch.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[
        DocExample {
            title: "Octave stack",
            code: "$sine($spread('c3', 'c6', 4)).out()",
        },
        DocExample {
            title: "Cluster bunched toward the top",
            code: "$saw($spread('c3', 'c4', 6, { bias: 3 })).out()",
        },
    ],
};

#[module(
    name = "$spread",
    docs = SPREAD_DOCS,
    channels_param = "count",
    args(min, max, count)
)]
pub struct Spread {
    outputs: SpreadOutputs,
    params: SpreadParams,
}

impl Spread {
    fn update(&mut self, _sample_rate: f32) {
        let count = self.channel_count().max(1);
        let min_val = self.params.min.get_value();
        let max_val = self.params.max.get_value();
        let bias = self.params.bias.value_or(0.0);

        // Compute the power curve exponent from bias.
        // bias = 0 → exponent = 1 (linear)
        // bias > 0 → exponent < 1 (values shift toward max)
        // bias < 0 → exponent > 1 (values shift toward min)
        let exponent = (2.0_f32).powf(-bias);

        for i in 0..count {
            let t = if count == 1 {
                0.5_f32
            } else {
                i as f32 / (count - 1) as f32
            };

            // Apply bias curve
            let t_biased = t.powf(exponent);

            let value = min_val + (max_val - min_val) * t_biased;
            self.outputs.sample.set(i, value);
        }
    }
}

message_handlers!(impl Spread {});
