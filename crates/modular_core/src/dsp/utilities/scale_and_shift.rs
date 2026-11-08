use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ScaleAndShiftParams {
    #[input("signal to scale and shift", type = audio)]
    input: PolySignal,
    #[input(
        "gain: 5 unity, 0 silence, 10 double, negative inverts",
        type = control,
        default = 5.0,
        control_range = (0.0, 10.0),
    )]
    #[deserr(default)]
    scale: Option<PolySignal>,
    #[input("offset added after scaling, in volts", type = control, control_range = (-5.0, 5.0))]
    #[deserr(default)]
    shift: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ScaleAndShiftOutputs {
    #[output("output", "the scaled and shifted signal", default)]
    sample: PolyOutput,
}

const SCALE_AND_SHIFT_DOCS: ModuleDocs = ModuleDocs {
    summary: "Attenuverter and offset",
    description: "Turns a signal up, down or upside down, then adds an offset: the \
                  attenuverter-and-offset found on most modular utility panels.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Filter wobble set by depth and centre",
        code: "$saw('c2').$.lpf($scaleAndShift($sine('0.5hz'), 1.5, 1), 2).out()",
    }],
};

#[module(
    name = "$scaleAndShift",
    docs = SCALE_AND_SHIFT_DOCS,
    args(input, scale, shift),
    block
)]
pub struct ScaleAndShift {
    outputs: ScaleAndShiftOutputs,
    params: ScaleAndShiftParams,
    channel_state: Box<[ChannelState]>,
}

#[derive(Clone, Copy, Default)]
struct ChannelState {
    scale: SwapGlide,
    shift: SwapGlide,
}

/// `input` scaled (5 V is unity gain) and then offset.
#[inline(always)]
fn scale_and_shift(input: f32, scale: f32, shift: f32) -> f32 {
    input * (scale / 5.0) + shift
}

impl ScaleAndShift {
    /// Outside a glide, reads each param raw, exactly as `process_block` does.
    fn update(&mut self, sample_rate: f32) {
        for i in 0..self.channel_count() {
            let state = &mut self.channel_state[i];
            let scale = glided(
                &mut state.scale,
                &self.params.scale,
                ScaleAndShiftParams::SCALE_DEFAULT,
                i,
                sample_rate,
            );
            let shift = glided(
                &mut state.shift,
                &self.params.shift,
                ScaleAndShiftParams::SHIFT_DEFAULT,
                i,
                sample_rate,
            );
            let value = scale_and_shift(self.params.input.get_value(i), scale, shift);
            self.outputs.sample.set(i, value);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut ScaleAndShiftBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let (scale, shift) = (&self.params.scale, &self.params.shift);
        let gliding = self.channel_state[..channels]
            .iter()
            .enumerate()
            .any(|(i, cs)| {
                cs.scale.will_glide(scale.glide_mark(i)) || cs.shift.will_glide(shift.glide_mark(i))
            });
        if gliding {
            return None;
        }
        let mut scratch = Default::default();
        let inputs = self
            .params
            .block_inputs(range.clone(), channels, &mut scratch)?;
        let last = range.end - 1;
        block_input::map(
            &mut outputs.sample,
            range,
            [inputs.input, inputs.scale, inputs.shift],
            |[x, scale, shift]| scale_and_shift(x, scale, shift),
        );
        for (i, cs) in self.channel_state[..channels].iter_mut().enumerate() {
            cs.scale
                .settle(inputs.scale.row(last)[i], scale.glide_mark(i));
            cs.shift
                .settle(inputs.shift.row(last)[i], shift.glide_mark(i));
        }
        Some(())
    }
}

/// The glided value of an optional param at channel `ch`; the raw value while
/// no glide runs.
#[inline(always)]
fn glided(
    glide: &mut SwapGlide,
    param: &Option<PolySignal>,
    default: f32,
    ch: usize,
    sample_rate: f32,
) -> f32 {
    glide.follow(
        param.value_or(ch, default),
        param.glide_mark(ch),
        sample_rate,
    )
}

message_handlers!(impl ScaleAndShift {});
