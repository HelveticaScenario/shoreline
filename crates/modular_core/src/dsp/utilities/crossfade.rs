use deserr::Deserr;
use schemars::JsonSchema;

use std::ops::Range;

use crate::block_input::{self, InputScratch};
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
use crate::reference_category::ReferenceCategory;
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct CrossfadeParams {
    #[input("signal heard at a mix of 0", type = audio, bounds)]
    a: PolySignal,
    #[input("signal heard at a mix of 5", type = audio, bounds)]
    b: PolySignal,
    #[input(
        "position between `a` and `b`",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    mix: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CrossfadeOutputs {
    #[output("output", "`a` faded into `b` by `mix`", type = cv, bounds = input, default)]
    sample: PolyOutput,
}

const CROSSFADE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Linear crossfade",
    description: "Fades linearly from `a` to `b` as `mix` rises, lane by lane, \
                  for audio or control voltages alike; the narrower of the two \
                  cycles across the wider. Modulate `mix` to sweep between two \
                  sounds, two LFOs or two melodies.",
    categories: &[ReferenceCategory::Mixer, ReferenceCategory::Math],
    sections: &[],
    examples: &[
        DocExample {
            title: "Morph between two oscillators",
            code: r#"
                lead: $crossfade(
                  $saw('c3'),
                  $pulse('c3'),
                  $sine('0.2hz').range(0, 5),
                )
            "#,
        },
        DocExample {
            title: "Blend two LFOs on a filter sweep",
            code: r#"
                bass: $saw('c2').bpf(
                  $sine('0.5hz')
                    .crossfade($saw('3hz'), 1)
                    .range('c4', 'c6'),
                  3,
                )
            "#,
        },
    ],
};

#[module(name = "$crossfade", docs = CROSSFADE_DOCS, args(a, b, mix), block)]
pub struct Crossfade {
    outputs: CrossfadeOutputs,
    params: CrossfadeParams,
    lane_state: Box<[SwapGlide]>,
}

/// How far along the fade a `mix` sits: 0 all `a`, 1 all `b`; `mix` past
/// either end holds there. Edits glide this position, so a jump from past one
/// end starts moving at once instead of waiting out the overshoot. A glide
/// onto a moving `mix` rides on it, so the glided position is clamped again.
#[inline(always)]
fn position(mix: f32) -> f32 {
    (mix / 5.0).clamp(0.0, 1.0)
}

#[inline(always)]
fn blend(a: f32, b: f32, position: f32) -> f32 {
    a * (1.0 - position) + b * position
}

impl Crossfade {
    fn update(&mut self, sample_rate: f32) {
        let mix = &self.params.mix;
        for i in 0..self.lane_count() {
            let t = self.lane_state[i].follow(
                position(mix.get_value(i)),
                mix.glide_mark(i),
                sample_rate,
            );
            let value = blend(
                self.params.a.get_value(i),
                self.params.b.get_value(i),
                t.clamp(0.0, 1.0),
            );
            self.outputs.sample.set(i, value);
            if self.outputs.sample.stores_bounds() {
                let (a, b) = (self.params.a.get_bounds(i), self.params.b.get_bounds(i));
                self.outputs.sample.set_bounds(i, a.hull(b));
            }
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut CrossfadeBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let glides = &mut self.lane_state[..lanes];
        let mix = &self.params.mix;
        if mix.glides_pending(glides) {
            return None;
        }
        let (mut s0, mut s1, mut s2) = (
            InputScratch::new(),
            InputScratch::new(),
            InputScratch::new(),
        );
        let a = self.params.a.block_input(range.clone(), lanes, &mut s0)?;
        let b = self.params.b.block_input(range.clone(), lanes, &mut s1)?;
        let amount = mix.block_input(range.clone(), lanes, &mut s2)?;
        let (a_signal, b_signal) = (&self.params.a, &self.params.b);
        outputs.sample.fill_bounds_with(range.clone(), |lane| {
            let a = a_signal.block_bounds(range.clone(), lane)?;
            Some(a.hull(b_signal.block_bounds(range.clone(), lane)?))
        })?;
        for (i, (glide, &last)) in glides.iter_mut().zip(amount.row(range.end - 1)).enumerate() {
            glide.settle(position(last), mix.glide_mark(i));
        }
        block_input::map(&mut outputs.sample, range, [a, b, amount], |[a, b, mix]| {
            blend(a, b, position(mix))
        });
        Some(())
    }
}

message_handlers!(impl Crossfade {});
