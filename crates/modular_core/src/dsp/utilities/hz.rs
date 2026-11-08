use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input;
use crate::dsp::utils::hz_to_voct;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};

/// Lowest frequency converted, keeping `hz_to_voct` away from log2(≤0).
const MIN_HZ: f32 = 1e-4;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct HzParams {
    #[input("frequency in Hz", type = control, range = (0.0, ..), control_range = (0.0, 1000.0))]
    input: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct HzOutputs {
    #[output("output", "the frequency as a V/Oct pitch", default)]
    sample: PolyOutput,
}

/// The V/Oct pitch of a frequency in Hz, with frequencies at or below
/// `MIN_HZ` read as `MIN_HZ`.
#[inline(always)]
fn hz(frequency: f32) -> f32 {
    hz_to_voct(frequency.max(MIN_HZ))
}

const HZ_DOCS: ModuleDocs = ModuleDocs {
    summary: "Hz to V/Oct",
    description: "Converts a frequency in Hz to a V/Oct pitch. Backs `$hz()` when its \
                  argument is a signal.",
    categories: &[ModuleCategory::Pitch],
    sections: &[],
    examples: &[DocExample {
        title: "Sweep set in Hz",
        code: "$sine($hz($sine('0.2hz').range(200, 800))).out()",
    }],
};

#[module(name = "_hz", docs = HZ_DOCS, args(input), block)]
pub struct Hz {
    outputs: HzOutputs,
    params: HzParams,
}

impl Hz {
    fn update(&mut self, _sample_rate: f32) {
        for i in 0..self.channel_count() {
            let value = hz(self.params.input.get_value(i));
            self.outputs.sample.set(i, value);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut HzBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let mut scratch = Default::default();
        let inputs = self
            .params
            .block_inputs(range.clone(), self.channel_count(), &mut scratch)?;
        block_input::map(&mut outputs.sample, range, [inputs.input], |[f]| hz(f));
        Some(())
    }
}

message_handlers!(impl Hz {});
