use deserr::Deserr;
use schemars::JsonSchema;
use simple_easing;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{LaneSumExt, PolySignal};
use crate::types::InterpolationType;
use crate::{LaneSum, PolyOutput};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct KeyframesParams {
    #[input("[value, position] pairs, positions from 0 to 1 in ascending order")]
    keyframes: Vec<(PolySignal, f32)>,
    #[input("position along the keyframes", type = control, range = (0.0, 1.0))]
    #[default_connection(module = RootClock, port = "playhead", lane = 0)]
    #[deserr(default)]
    playhead: Option<LaneSum>,
    #[input("curve shape between neighbouring keyframes")]
    #[serde(default)]
    #[deserr(default)]
    interpolation_type: InterpolationType,
}

fn derive_keyframes_lane_count(params: &KeyframesParams) -> usize {
    params
        .keyframes
        .iter()
        .map(|(sig, _)| sig.lanes())
        .max()
        .unwrap_or(0)
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct KeyframesOutputs {
    #[output(
        "output",
        "the value at the current position, curved between keyframes",
        type = cv,
        default
    )]
    sample: PolyOutput,
}

const KEYFRAMES_DOCS: ModuleDocs = ModuleDocs {
    summary: "Keyframed automation curve",
    description: "Draws a curve through `[value, position]` keyframes and plays it along the \
                  bar, for automation that rises and falls at set points: a filter that opens \
                  over the bar, a swell into the downbeat.",
    categories: &[ModuleCategory::Sequencer, ModuleCategory::Modulation],
    sections: &[],
    examples: &[DocExample {
        title: "Filter that opens and closes each bar",
        code: r#"
            const cutoff = $keyframes(
              [
                ['c3', 0],
                ['c6', 0.75],
                ['c3', 1],
              ],
              { interpolationType: 'cubicIn' },
            )
            $saw('c2').lpf(cutoff, 3).out()
        "#,
    }],
};

#[module(name = "$keyframes", docs = KEYFRAMES_DOCS, args(keyframes), lanes_derive = derive_keyframes_lane_count)]
pub struct Keyframes {
    outputs: KeyframesOutputs,
    params: KeyframesParams,
}

impl Keyframes {
    fn update(&mut self, _sample_rate: f32) {
        let lane_count = self.lane_count();
        let channels = self.channels();

        for lane in 0..lane_count {
            let t = self
                .params
                .playhead
                .side_or(lane % channels, 0.0)
                .clamp(0.0, 1.0);
            // Single keyframe: always return its value
            if self.params.keyframes.len() == 1 {
                self.outputs
                    .sample
                    .set(lane, self.params.keyframes[0].0.get_value(lane));
            }
            // Clamp to first/last keyframe times
            let first = &self.params.keyframes[0];
            if t <= first.1 {
                self.outputs.sample.set(lane, first.0.get_value(lane));
                continue;
            }
            let last = self.params.keyframes.last().unwrap();
            if t >= last.1 {
                self.outputs.sample.set(lane, last.0.get_value(lane));
                continue;
            }

            // Find the segment [curr, next] such that curr.time <= t <= next.time
            // Use partition_point to find the first keyframe with time > t
            // Then back up one to get the last keyframe with time <= t
            let idx = self.params.keyframes.partition_point(|kf| kf.1 <= t);

            // partition_point returns the index of the first element > t
            // So idx-1 is the last element <= t, which is the start of our interpolation segment
            let idx = if idx > 0 { idx - 1 } else { 0 };

            // Ensure idx is valid for the segment [idx, idx+1]
            let idx = idx.min(self.params.keyframes.len() - 2);

            let curr = &self.params.keyframes[idx];
            let next = &self.params.keyframes[idx + 1];

            let curr_value = curr.0.get_value(lane);
            let next_value = next.0.get_value(lane);

            let time_range = (next.1 - curr.1).max(f32::EPSILON);
            let mut local_t = (t - curr.1) / time_range;
            local_t = local_t.clamp(0.0, 1.0);

            self.outputs.sample.set(
                lane,
                apply_easing_interpolation(
                    self.params.interpolation_type,
                    curr_value,
                    next_value,
                    local_t,
                ),
            );
        }
    }
}

fn apply_easing_interpolation(
    interpolation_type: InterpolationType,
    curr_value: f32,
    next_value: f32,
    local_t: f32,
) -> f32 {
    match interpolation_type {
        InterpolationType::Linear => {
            curr_value + (next_value - curr_value) * simple_easing::linear(local_t)
        }
        InterpolationType::Step => curr_value,
        InterpolationType::SineIn => {
            curr_value + (next_value - curr_value) * simple_easing::sine_in(local_t)
        }
        InterpolationType::SineOut => {
            curr_value + (next_value - curr_value) * simple_easing::sine_out(local_t)
        }
        InterpolationType::SineInOut => {
            curr_value + (next_value - curr_value) * simple_easing::sine_in_out(local_t)
        }
        InterpolationType::QuadIn => {
            curr_value + (next_value - curr_value) * simple_easing::quad_in(local_t)
        }
        InterpolationType::QuadOut => {
            curr_value + (next_value - curr_value) * simple_easing::quad_out(local_t)
        }
        InterpolationType::QuadInOut => {
            curr_value + (next_value - curr_value) * simple_easing::quad_in_out(local_t)
        }
        InterpolationType::CubicIn => {
            curr_value + (next_value - curr_value) * simple_easing::cubic_in(local_t)
        }
        InterpolationType::CubicOut => {
            curr_value + (next_value - curr_value) * simple_easing::cubic_out(local_t)
        }
        InterpolationType::CubicInOut => {
            curr_value + (next_value - curr_value) * simple_easing::cubic_in_out(local_t)
        }
        InterpolationType::QuartIn => {
            curr_value + (next_value - curr_value) * simple_easing::quart_in(local_t)
        }
        InterpolationType::QuartOut => {
            curr_value + (next_value - curr_value) * simple_easing::quart_out(local_t)
        }
        InterpolationType::QuartInOut => {
            curr_value + (next_value - curr_value) * simple_easing::quart_in_out(local_t)
        }
        InterpolationType::QuintIn => {
            curr_value + (next_value - curr_value) * simple_easing::quint_in(local_t)
        }
        InterpolationType::QuintOut => {
            curr_value + (next_value - curr_value) * simple_easing::quint_out(local_t)
        }
        InterpolationType::QuintInOut => {
            curr_value + (next_value - curr_value) * simple_easing::quint_in_out(local_t)
        }
        InterpolationType::ExpoIn => {
            curr_value + (next_value - curr_value) * simple_easing::expo_in(local_t)
        }
        InterpolationType::ExpoOut => {
            curr_value + (next_value - curr_value) * simple_easing::expo_out(local_t)
        }
        InterpolationType::ExpoInOut => {
            curr_value + (next_value - curr_value) * simple_easing::expo_in_out(local_t)
        }
        InterpolationType::CircIn => {
            curr_value + (next_value - curr_value) * simple_easing::circ_in(local_t)
        }
        InterpolationType::CircOut => {
            curr_value + (next_value - curr_value) * simple_easing::circ_out(local_t)
        }
        InterpolationType::CircInOut => {
            curr_value + (next_value - curr_value) * simple_easing::circ_in_out(local_t)
        }
        InterpolationType::BounceIn => {
            curr_value + (next_value - curr_value) * simple_easing::bounce_in(local_t)
        }
        InterpolationType::BounceOut => {
            curr_value + (next_value - curr_value) * simple_easing::bounce_out(local_t)
        }
        InterpolationType::BounceInOut => {
            curr_value + (next_value - curr_value) * simple_easing::bounce_in_out(local_t)
        }
    }
}

message_handlers!(impl Keyframes {});
