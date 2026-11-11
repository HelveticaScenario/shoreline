use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input::{self, InputScratch};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct CurveParams {
    #[input("signal to bend", type = audio)]
    input: PolySignal,
    #[input(
        "power the signal is raised to: 0 jumps straight to ±5 V, 1 leaves it unchanged, above 1 pulls the middle toward 0",
        type = control,
        default = 1.0,
        range = (0.0, ..),
        control_range = (0.0, 10.0),
    )]
    exp: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CurveOutputs {
    #[output("output", "the bent signal", type = cv, default)]
    sample: PolyOutput,
}

const CURVE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Bend a signal's response",
    description: "Raises a signal to a power while keeping its sign and its ±5 V \
                  extremes, so straight ramps become curves. Use it to give an \
                  envelope or LFO a snappier or lazier shape.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Snappy filter envelope",
        code: r#"
            $saw('c2')
              .$.lpf(
                $curve(
                  $adsr($clock.o.beatTrigger, { decay: 0.3 }),
                  3,
                ).range('c3', 'c7', 0, 5),
                2,
              )
              .out()
        "#,
    }],
};

#[module(name = "$curve", docs = CURVE_DOCS, args(input, exp), block)]
pub struct Curve {
    outputs: CurveOutputs,
    params: CurveParams,
    channel_state: Box<[SwapGlide]>,
}

/// `x` bent by `exp` across ±5 V, keeping its sign; a negative exponent
/// counts as 0.
#[inline(always)]
fn curve(x: f32, exp: f32) -> f32 {
    let normalized = (x.abs() / 5.0).max(0.0);
    x.signum() * 5.0 * normalized.powf(exp.max(0.0))
}

impl Curve {
    fn update(&mut self, sample_rate: f32) {
        let exp = &self.params.exp;
        for i in 0..self.channel_count() {
            let exp_value =
                self.channel_state[i].follow(exp.get_value(i), exp.glide_mark(i), sample_rate);
            let value = curve(self.params.input.get_value(i), exp_value);
            self.outputs.sample.set(i, value);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut CurveBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let glides = &mut self.channel_state[..channels];
        let exp = &self.params.exp;
        if exp.glides_pending(glides) {
            return None;
        }
        let (mut s0, mut s1) = (InputScratch::new(), InputScratch::new());
        let input = self
            .params
            .input
            .block_input(range.clone(), channels, &mut s0)?;
        let exp_input = exp.block_input(range.clone(), channels, &mut s1)?;
        exp.settle_glides(glides, exp_input.row(range.end - 1));
        block_input::map(
            &mut outputs.sample,
            range,
            [input, exp_input],
            |[x, exp]| curve(x, exp),
        );
        Some(())
    }
}

message_handlers!(impl Curve {});
