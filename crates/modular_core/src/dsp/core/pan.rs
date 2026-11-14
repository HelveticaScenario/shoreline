//! Place each lane in the stereo field.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::sanitize;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{LaneSum, LaneSumExt, PairSignal, PolyOutput, PolySignal, PolySignalExt};
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct PanParams {
    #[input("the signal to place in the stereo field", type = audio)]
    input: PairSignal,
    #[input(
        "where each lane sits: -5 left, 0 centre, +5 right",
        type = control,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    pan: Option<PolySignal>,
    #[input(
        "how far the lanes fan out around their pan positions, the first to the left and the last to the right",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    spread: Option<LaneSum>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PanOutputs {
    #[output("output", "each lane placed in the stereo field", type = cv, default)]
    sample: PolyOutput,
}

const PAN_DOCS: ModuleDocs = ModuleDocs {
    summary: "Place lanes in the stereo field",
    description: "Pans each lane, with equal-power gains that keep a centred lane at its level. \
                  A stereo lane moves as a whole: panning right carries its left side \
                  across. Raise `spread` to fan the lanes of a chord out from the centre.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[
        DocExample {
            title: "Chord fanned across the stereo field",
            code: "$pan($saw($chord('C3[m9]')), 0, { spread: 4 }).out()",
        },
        DocExample {
            title: "An autopanned bass",
            code: "$pan($saw('c2'), $sine('0.2hz').range(-3, 3)).out()",
        },
    ],
};

#[module(name = "$pan", docs = PAN_DOCS, args(input, pan), stereo = stereo)]
pub struct Pan {
    outputs: PanOutputs,
    params: PanParams,
    state: PanState,
    lane_state: Box<[SwapGlide]>,
}

#[derive(Default)]
struct PanState {
    spread: SwapGlide,
}

/// The `[left, right]` gains of a pair panned to `p` in -1..1: the side
/// toward which it moves stays at unity, and the far side crosses over. A pair
/// whose sides match takes Live's equal-power law, unity at the centre and
/// √2 at either edge.
#[inline]
pub(crate) fn pan_pair([left, right]: [f32; 2], p: f32) -> [f32; 2] {
    let p = p.clamp(-1.0, 1.0);
    if p >= 0.0 {
        [
            left * (1.0 - p).sqrt(),
            right + left * ((1.0 + p).sqrt() - 1.0),
        ]
    } else {
        [
            left + right * ((1.0 - p).sqrt() - 1.0),
            right * (1.0 + p).sqrt(),
        ]
    }
}

impl Pan {
    fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        let spread = self.state.spread.follow(
            p.spread.value_or(0.0).clamp(0.0, 5.0),
            p.spread.glide_mark(),
            sample_rate,
        );
        // Lane `l` side `c` is lane `l * 2 + c`.
        let lanes = self.lane_count() / 2;
        for (lane, glide) in self.lane_state.iter_mut().enumerate().take(lanes) {
            // A stereo pan control places each lane at its sides' average.
            let base = 0.5 * (p.pan.value_or_zero(lane * 2) + p.pan.value_or_zero(lane * 2 + 1));
            let base = glide.follow(
                base.clamp(-5.0, 5.0),
                p.pan.glide_mark(lane * 2),
                sample_rate,
            );
            let offset = if lanes > 1 {
                (lane as f32 / (lanes - 1) as f32 - 0.5) * 2.0 * spread
            } else {
                0.0
            };
            let pair = [p.input.get_value(lane, 0), p.input.get_value(lane, 1)];
            let [left, right] = pan_pair(pair, (base + offset) / 5.0);
            self.outputs.sample.set(lane * 2, sanitize(left));
            self.outputs.sample.set(lane * 2 + 1, sanitize(right));
        }
    }
}

message_handlers!(impl Pan {});

#[cfg(test)]
mod tests {
    use super::pan_pair;

    #[test]
    fn a_matched_pair_takes_the_equal_power_law() {
        for p in [-1.0f32, -0.5, 0.0, 0.3, 1.0] {
            let [l, r] = pan_pair([1.0, 1.0], p);
            assert!((l - (1.0 - p).sqrt()).abs() < 1e-6, "{p}: {l}");
            assert!((r - (1.0 + p).sqrt()).abs() < 1e-6, "{p}: {r}");
        }
    }

    #[test]
    fn the_centre_leaves_a_pair_as_it_is() {
        assert_eq!(pan_pair([0.3, -0.7], 0.0), [0.3, -0.7]);
    }

    #[test]
    fn panning_right_carries_the_left_side_across() {
        let [l, r] = pan_pair([1.0, 0.0], 1.0);
        assert_eq!(l, 0.0);
        assert!(r > 0.0);
        assert_eq!(pan_pair([0.0, 1.0], 1.0), [0.0, 1.0]);
    }
}
