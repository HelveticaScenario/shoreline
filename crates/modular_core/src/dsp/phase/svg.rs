//! `$svg` — traces a line drawing with a phase, for an XY scope.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::drawing::Drawing;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};
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
    )]
    phase: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SvgOutputs {
    #[output("x", "the pen's left–right position", type = cv, default, range = (-5.0, 5.0))]
    x: PolyOutput,
    #[output("y", "the pen's up–down position", type = cv, range = (-5.0, 5.0))]
    y: PolyOutput,
}

const SVG_DOCS: ModuleDocs = ModuleDocs {
    summary: "Trace a line drawing",
    description: "Moves a pen along every line of an SVG drawing as the phase runs from 0 to \
                  1, giving its position as `x` and `y` for `$scopeXY` or a vector display. \
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
            $scopeXY(pen._x, pen._y)
        "#,
    }],
};

#[module(name = "$svg", docs = SVG_DOCS, args(file, phase), experimental)]
pub struct Svg {
    outputs: SvgOutputs,
    params: SvgParams,
}

impl Svg {
    fn update(&mut self, _sample_rate: f32) {
        for lane in 0..self.lane_count() {
            let [x, y] = self.params.file.trace(self.params.phase.get_value(lane));
            self.outputs.x.set(lane, x);
            self.outputs.y.set(lane, y);
        }
    }
}

message_handlers!(impl Svg {});
