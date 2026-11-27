//! `$svg` — traces a line drawing with a phase, for an XY scope.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::drawing::Drawing;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PairSignal, PolyOutput};
use crate::reference_category::ReferenceCategory;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SvgParams {
    #[input("the SVG drawing to trace")]
    file: Drawing,
    #[input(
        "how far along the drawing the pen is, usually from `$ramp`; one cycle traces it once",
        type = audio,
        range = (0.0, 1.0),
        fold,
    )]
    phase: PairSignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SvgOutputs {
    #[output(
        "output",
        "the pen's position: left–right on the left side, up–down on the right",
        type = cv,
        default,
        range = (-5.0, 5.0),
    )]
    sample: PolyOutput,
}

const SVG_DOCS: ModuleDocs = ModuleDocs {
    summary: "Trace a line drawing",
    description: "Moves a pen along every line of an SVG drawing as the phase runs from 0 to \
                  1, giving its position as a stereo signal, left–right on the left side and \
                  up–down on the right, for `$scopeXY` or a vector display. \
                  The drawing is fitted to ±5 V, and the pen jumps between separate strokes \
                  without drawing, so a drawing made of one continuous line traces cleanest.",
    categories: &[ReferenceCategory::Phase],
    sections: &[],
    examples: &[DocExample {
        title: "Draw a plover",
        code: r#"
            const pen = $svg(
              'builtin/drawings/plover.svg',
              $ramp('20hz'),
            )
            $scopeXY($left(pen), $right(pen))
        "#,
    }],
};

/// One stereo lane per phase lane.
fn svg_derive_lane_count(params: &SvgParams) -> usize {
    params.phase.lanes().max(1)
}

#[module(
    name = "$svg",
    docs = SVG_DOCS,
    lanes_derive = svg_derive_lane_count,
    args(file, phase),
    stereo = stereo,
    experimental,
)]
pub struct Svg {
    outputs: SvgOutputs,
    params: SvgParams,
}

impl Svg {
    fn update(&mut self, _sample_rate: f32) {
        // Lane `l` side `c` is lane `l * 2 + c`.
        for lane in 0..self.lane_count() / 2 {
            let [x, y] = self.params.file.trace(self.params.phase.get_folded(lane));
            self.outputs.sample.set(lane * 2, x);
            self.outputs.sample.set(lane * 2 + 1, y);
        }
    }
}

message_handlers!(impl Svg {});
