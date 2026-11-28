use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input;
use crate::dsp::utils::voct_to_hz;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use crate::reference_category::ReferenceCategory;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct VToHzParams {
    #[input("pitch to convert", type = pitch)]
    input: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct VToHzOutputs {
    #[output("output", "the pitch's frequency in Hz", type = cv, default)]
    sample: PolyOutput,
}

const V_TO_HZ_DOCS: ModuleDocs = ModuleDocs {
    summary: "V/Oct to Hz",
    description: "Converts a V/Oct pitch to its frequency in Hz, so `'200hz'` becomes \
                  200 and `'a4'` becomes 440. Feed it to `addHz` to detune by a \
                  frequency set as a pitch, such as an Hz slider. The inverse of `$hzToV()`.",
    categories: &[ReferenceCategory::Pitch],
    sections: &[],
    examples: &[DocExample {
        title: "Detune by an Hz slider",
        code: r#"
            const pitch = $`c3 eb3 g3 bb2`
            melody: $saw(pitch)
            melody2: $saw(
              pitch.addHz(
                $slider('offset', '1hz', '1hz', '1000hz').vToHz(),
              ),
            )
        "#,
    }],
};

#[module(name = "$vToHz", docs = V_TO_HZ_DOCS, args(input), block)]
pub struct VToHz {
    outputs: VToHzOutputs,
    params: VToHzParams,
}

impl VToHz {
    fn update(&mut self, _sample_rate: f32) {
        for i in 0..self.lane_count() {
            let value = voct_to_hz(self.params.input.get_value(i));
            self.outputs.sample.set(i, value);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut VToHzBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let mut scratch = Default::default();
        let inputs = self
            .params
            .block_inputs(range.clone(), self.lane_count(), &mut scratch)?;
        block_input::map(&mut outputs.sample, range, [inputs.input], |[v]| {
            voct_to_hz(v)
        });
        Some(())
    }
}

message_handlers!(impl VToHz {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::utils::{C4_HZ_F32, hz_to_voct};

    #[test]
    fn converts_pitch_to_hz() {
        assert!((voct_to_hz(0.0) - C4_HZ_F32).abs() < 1e-3);
        assert!((voct_to_hz(hz_to_voct(200.0)) - 200.0).abs() < 1e-3);
    }
}
