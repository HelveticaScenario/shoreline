//! `$svg` — traces a line drawing with a phase, for an XY scope.

use deserr::Deserr;
use schemars::JsonSchema;
use serde::Serialize;

use crate::drawing::Drawing;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PairSignal, PolyOutput, PolySignal, PolySignalExt};
use crate::reference_category::ReferenceCategory;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SvgParams {
    #[input("the SVG drawing to trace; with two or more top-level groups, each is a frame")]
    file: Drawing,
    #[input(
        "how far along the drawing the pen is, usually from `$ramp`; one cycle traces it once",
        type = audio,
        range = (0.0, 1.0),
        fold,
    )]
    phase: PairSignal,
    #[input(
        "how far into an animated drawing to show, in seconds; past the end it starts again, and before the start it counts back from the end",
        type = control,
        default = 0.0,
        control_range = (0.0, 10.0),
        fold,
    )]
    #[deserr(default)]
    frame: Option<PolySignal>,
    #[input("when a change of frame shows")]
    #[serde(rename = "switch", default)]
    #[deserr(rename = "switch", default)]
    r#switch: FrameSwitch,
}

/// When `$svg` shows a new frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, Serialize, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum FrameSwitch {
    /// at the end of a pass, as the phase wraps, so every pass draws one whole frame
    #[default]
    Wrap,
    /// at once, so a pass can draw part of one frame and part of the next
    Now,
}

/// Each lane's frame, held until its phase wraps.
#[derive(Default)]
struct SvgLane {
    last_phase: f32,
    frame: Option<i64>,
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
                  without drawing, so a drawing made of one continuous line traces cleanest. \
                  A drawing with several top-level groups (an editor's layers) is an \
                  animation, one frame per group, played by `frame` at the root's \
                  `data-fps` frames a second, or one a second without it.",
    categories: &[ReferenceCategory::Phase],
    sections: &[],
    examples: &[
        DocExample {
            title: "Draw a plover",
            code: r#"
            const pen = $svg(
              'builtin/drawings/plover.svg',
              $ramp('20hz'),
            )
            $scopeXY(pen)
        "#,
        },
        DocExample {
            title: "Play an animation",
            code: r#"
            const pen = $svg(
              'builtin/drawings/shapes.svg',
              $ramp('50hz'),
              { frame: $ramp('0.25hz').range(0, 4) },
            )
            $scopeXY(pen)
        "#,
        },
    ],
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
    lane_state: Box<[SvgLane]>,
}

impl Svg {
    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count() / 2;
        let p = &self.params;
        // Lane `l` side `c` is lane `l * 2 + c`.
        for (lane, state) in self.lane_state.iter_mut().enumerate().take(lanes) {
            let phase = p.phase.get_folded(lane);
            let wrapped = phase - phase.floor();
            let wanted = p
                .file
                .frame_at(p.frame.folded_or(lane, SvgParams::FRAME_DEFAULT));
            let frame = match (p.r#switch, state.frame) {
                // A wrap either way is a jump of over half a pass.
                (FrameSwitch::Wrap, Some(held)) if (wrapped - state.last_phase).abs() <= 0.5 => {
                    held
                }
                _ => wanted,
            };
            state.frame = Some(frame);
            state.last_phase = wrapped;
            let [x, y] = p.file.trace(phase, frame);
            self.outputs.sample.set(lane * 2, x);
            self.outputs.sample.set(lane * 2 + 1, y);
        }
    }
}

message_handlers!(impl Svg {});
