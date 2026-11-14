//! Join, split and fold the two sides of a stereo signal.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PairSignal, PolyOutput, PolySignal};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct StereoParams {
    #[input("the left side", type = audio, fold)]
    left: PairSignal,
    #[input("the right side", type = audio, fold)]
    right: PairSignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct StereoOutputs {
    #[output("output", "the stereo signal", type = cv, default)]
    sample: PolyOutput,
}

const STEREO_DOCS: ModuleDocs = ModuleDocs {
    summary: "Join two sides into stereo",
    description: "Makes a stereo signal from a left and a right signal, lane by lane. A \
                  stereo input is folded to mono first.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "Two sines a fifth apart",
        code: "$stereo($sine('c3'), $sine('g3')).out()",
    }],
};

#[module(
    name = "$stereo",
    docs = STEREO_DOCS,
    args(left, right),
    stereo = stereo,
)]
pub struct Stereo {
    outputs: StereoOutputs,
    params: StereoParams,
}

impl Stereo {
    fn update(&mut self, _sample_rate: f32) {
        // Lane `l` side `c` is lane `l * 2 + c`.
        for lane in 0..self.lane_count() {
            let input = if lane % 2 == 0 {
                &self.params.left
            } else {
                &self.params.right
            };
            self.outputs.sample.set(lane, input.get_folded(lane / 2));
        }
    }
}

message_handlers!(impl Stereo {});

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MonoParams {
    #[input("the signal to fold", type = audio)]
    input: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MonoOutputs {
    #[output("output", "each lane's sides averaged", type = cv, default)]
    sample: PolyOutput,
}

const MONO_DOCS: ModuleDocs = ModuleDocs {
    summary: "Fold stereo to mono",
    description: "Averages the two sides of each lane, `(L + R) / 2`. A mono signal passes \
                  unchanged.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "A stereo pair as one voice",
        code: "$mono($stereo($sine('c3'), $saw('c3'))).out()",
    }],
};

#[module(name = "$mono", docs = MONO_DOCS, args(input), stereo = mono)]
pub struct Mono {
    outputs: MonoOutputs,
    params: MonoParams,
}

impl Mono {
    fn update(&mut self, _sample_rate: f32) {
        for lane in 0..self.lane_count() {
            self.outputs
                .sample
                .set(lane, self.params.input.get_value(lane));
        }
    }
}

message_handlers!(impl Mono {});

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SideParams {
    #[input("the stereo signal", type = audio)]
    input: PairSignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SideOutputs {
    #[output("output", "one side of each lane", type = cv, default)]
    sample: PolyOutput,
}

type LeftParams = SideParams;
type RightParams = SideParams;

const LEFT_DOCS: ModuleDocs = ModuleDocs {
    summary: "Left side of stereo",
    description: "The left side of each lane, as a mono signal. A mono signal passes \
                  unchanged.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "Swap the sides",
        code: r#"
            const pair = $stereo($sine('c3'), $saw('c3'))
            $stereo($right(pair), $left(pair)).out()
        "#,
    }],
};

#[module(
    name = "$left",
    docs = LEFT_DOCS,
    args(input),
    stereo = mono,
)]
pub struct Left {
    outputs: SideOutputs,
    params: SideParams,
}

impl Left {
    fn update(&mut self, _sample_rate: f32) {
        for lane in 0..self.lane_count() {
            self.outputs
                .sample
                .set(lane, self.params.input.get_value(lane, 0));
        }
    }
}

message_handlers!(impl Left {});

const RIGHT_DOCS: ModuleDocs = ModuleDocs {
    summary: "Right side of stereo",
    description: "The right side of each lane, as a mono signal. A mono signal passes \
                  unchanged.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "Only the right side",
        code: "$right($stereo($sine('c3'), $saw('c3'))).out()",
    }],
};

#[module(
    name = "$right",
    docs = RIGHT_DOCS,
    args(input),
    stereo = mono,
)]
pub struct Right {
    outputs: SideOutputs,
    params: SideParams,
}

impl Right {
    fn update(&mut self, _sample_rate: f32) {
        for lane in 0..self.lane_count() {
            self.outputs
                .sample
                .set(lane, self.params.input.get_value(lane, 1));
        }
    }
}

message_handlers!(impl Right {});

type MidParams = MonoParams;

const MID_DOCS: ModuleDocs = ModuleDocs {
    summary: "Mid of a stereo signal",
    description: "The mid of each lane, `(L + R) / 2`, as a mono signal: what both sides \
                  share. Pair it with `$side` and rebuild with `$fromMidSide`.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "Narrow a stereo signal",
        code: r#"
            const wide = $stereo($saw('c3'), $saw('g3'))
            $fromMidSide($mid(wide), $side(wide).mul(0.5)).out()
        "#,
    }],
};

#[module(name = "$mid", docs = MID_DOCS, args(input), stereo = mono)]
pub struct Mid {
    outputs: MonoOutputs,
    params: MidParams,
}

impl Mid {
    fn update(&mut self, _sample_rate: f32) {
        for lane in 0..self.lane_count() {
            self.outputs
                .sample
                .set(lane, self.params.input.get_value(lane));
        }
    }
}

message_handlers!(impl Mid {});

type SideOfParams = SideParams;

const SIDE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Side of a stereo signal",
    description: "The side of each lane, `(L - R) / 2`, as a mono signal: what makes it wide. \
                  A mono signal's side is silent.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "Only the width",
        code: "$side($stereo($saw('c3'), $saw('g3'))).out()",
    }],
};

#[module(name = "$side", docs = SIDE_DOCS, args(input), stereo = mono)]
pub struct SideOf {
    outputs: SideOutputs,
    params: SideOfParams,
}

impl SideOf {
    fn update(&mut self, _sample_rate: f32) {
        for lane in 0..self.lane_count() {
            let input = &self.params.input;
            self.outputs.sample.set(
                lane,
                0.5 * (input.get_value(lane, 0) - input.get_value(lane, 1)),
            );
        }
    }
}

message_handlers!(impl SideOf {});

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct FromMidSideParams {
    #[input("what both sides share", type = audio, fold)]
    mid: PairSignal,
    #[input("what makes the signal wide", type = audio, fold)]
    side: PairSignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct FromMidSideOutputs {
    #[output("output", "the stereo signal: mid plus side on the left, mid minus side on the right", type = cv, default)]
    sample: PolyOutput,
}

const FROM_MID_SIDE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Stereo from mid and side",
    description: "Rebuilds a stereo signal from its mid and side, lane by lane: \
                  `L = mid + side`, `R = mid - side`. A stereo input is folded to mono first.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "Widen a stereo signal",
        code: r#"
            const pair = $stereo($saw('c3'), $saw('c3').mul(0.8))
            $fromMidSide($mid(pair), $side(pair).mul(2)).out()
        "#,
    }],
};

#[module(name = "$fromMidSide", docs = FROM_MID_SIDE_DOCS, args(mid, side), stereo = stereo)]
pub struct FromMidSide {
    outputs: FromMidSideOutputs,
    params: FromMidSideParams,
}

impl FromMidSide {
    fn update(&mut self, _sample_rate: f32) {
        // Lane `l` side `c` is lane `l * 2 + c`.
        for lane in 0..self.lane_count() / 2 {
            let mid = self.params.mid.get_folded(lane);
            let side = self.params.side.get_folded(lane);
            self.outputs.sample.set(lane * 2, mid + side);
            self.outputs.sample.set(lane * 2 + 1, mid - side);
        }
    }
}

message_handlers!(impl FromMidSide {});
