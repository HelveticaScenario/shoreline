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
struct CrossfadeParams {
    #[input("signal heard at a mix of 0", type = audio, bounds)]
    dry: PolySignal,
    #[input("signal heard at a mix of 5", type = audio, bounds)]
    wet: PolySignal,
    #[input(
        "balance from dry to wet",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    mix: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CrossfadeOutputs {
    #[output("output", "the blend of dry and wet", type = cv, bounds = input, default)]
    sample: PolyOutput,
}

const CROSSFADE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Dry/wet crossfade",
    description: "Fades linearly from one signal to another, channel by channel; \
                  the narrower of the two cycles across the wider. Backs the \
                  `.wet()` and `.pipeMix()` signal methods.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Half-wet filter",
        code: "$saw('c2').lpf('400hz').wet(2.5).out()",
    }],
};

#[module(name = "_crossfade", docs = CROSSFADE_DOCS, args(dry, wet, mix), block)]
pub struct Crossfade {
    outputs: CrossfadeOutputs,
    params: CrossfadeParams,
    channel_state: Box<[SwapGlide]>,
}

/// How far along the fade a `mix` sits: 0 all dry, 1 all wet; `mix` past
/// either end holds there. Edits glide this position, so a jump from past one
/// end starts moving at once instead of waiting out the overshoot. A glide
/// onto a moving `mix` rides on it, so the glided position is clamped again.
#[inline(always)]
fn position(mix: f32) -> f32 {
    (mix / 5.0).clamp(0.0, 1.0)
}

#[inline(always)]
fn blend(dry: f32, wet: f32, position: f32) -> f32 {
    dry * (1.0 - position) + wet * position
}

impl Crossfade {
    fn update(&mut self, sample_rate: f32) {
        let mix = &self.params.mix;
        for i in 0..self.channel_count() {
            let t = self.channel_state[i].follow(
                position(mix.get_value(i)),
                mix.glide_mark(i),
                sample_rate,
            );
            let value = blend(
                self.params.dry.get_value(i),
                self.params.wet.get_value(i),
                t.clamp(0.0, 1.0),
            );
            self.outputs.sample.set(i, value);
            if self.outputs.sample.stores_bounds() {
                let (dry, wet) = (self.params.dry.get_bounds(i), self.params.wet.get_bounds(i));
                self.outputs.sample.set_bounds(i, dry.hull(wet));
            }
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut CrossfadeBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let glides = &mut self.channel_state[..channels];
        let mix = &self.params.mix;
        if mix.glides_pending(glides) {
            return None;
        }
        let (mut s0, mut s1, mut s2) = (
            InputScratch::new(),
            InputScratch::new(),
            InputScratch::new(),
        );
        let dry = self
            .params
            .dry
            .block_input(range.clone(), channels, &mut s0)?;
        let wet = self
            .params
            .wet
            .block_input(range.clone(), channels, &mut s1)?;
        let amount = mix.block_input(range.clone(), channels, &mut s2)?;
        let (dry_signal, wet_signal) = (&self.params.dry, &self.params.wet);
        outputs.sample.fill_bounds_with(range.clone(), |ch| {
            let dry = dry_signal.block_bounds(range.clone(), ch)?;
            Some(dry.hull(wet_signal.block_bounds(range.clone(), ch)?))
        })?;
        for (i, (glide, &last)) in glides.iter_mut().zip(amount.row(range.end - 1)).enumerate() {
            glide.settle(position(last), mix.glide_mark(i));
        }
        block_input::map(
            &mut outputs.sample,
            range,
            [dry, wet, amount],
            |[dry, wet, mix]| blend(dry, wet, position(mix)),
        );
        Some(())
    }
}

message_handlers!(impl Crossfade {});
