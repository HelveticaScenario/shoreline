//! Fold the copies of a unison stack into one stereo lane per note.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::core::pan::pan_pair;
use crate::dsp::utils::sanitize;
use crate::dsp::utils::unison_voices::{
    centred_position, detune_of_slot, unison_gain, unison_lane,
};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PORT_MAX_LANES, PairSignal, PolyOutput, PolySignal, PolySignalExt};
use crate::types::SwapGlide;

fn default_voices() -> usize {
    1
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct StereoFoldParams {
    #[input("the unison copies to fold, laid out as `$unison` makes them", type = audio)]
    input: PairSignal,
    #[input("copies per note, the same as `$unison`'s count", range = (1, 64))]
    #[serde(default = "default_voices")]
    #[deserr(default = default_voices())]
    voices: usize,
    #[input(
        "how far the copies fan out: 0 all centred, 5 the outer copies hard left and right",
        type = control,
        default = 5.0,
        range = (0.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    width: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct StereoFoldOutputs {
    #[output("output", "one stereo lane per note", type = cv, default)]
    sample: PolyOutput,
}

const STEREO_FOLD_DOCS: ModuleDocs = ModuleDocs {
    summary: "Spread unison copies in stereo",
    description: "Mixes each note's `$unison` copies down to one stereo lane, fanning them \
                  across the field the way `$supersaw` spreads its saws, so any oscillator \
                  can make a wide unison stack.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[
        DocExample {
            title: "Wide seven-voice saw",
            code: "lead: $saw($unison('c3', 7, 3)).stereoFold(7)",
        },
        DocExample {
            title: "Chord whose width breathes",
            code: r#"
                pad: $sine($unison($chord('C3[m9]'), 4, 2)).stereoFold(
                  4,
                  $sine('0.1hz').range(1, 5),
                )
            "#,
        },
    ],
};

fn voices(params: &StereoFoldParams) -> usize {
    params.voices.clamp(1, PORT_MAX_LANES)
}

/// One lane per note: every `voices` input lanes, rounding up so a stack
/// `$unison` cut short at the lane limit keeps its last notes.
fn stereo_fold_derive_lane_count(params: &StereoFoldParams) -> usize {
    params.input.lanes().div_ceil(voices(params)).max(1)
}

#[module(name = "$stereoFold", docs = STEREO_FOLD_DOCS, lanes_derive = stereo_fold_derive_lane_count, args(input, voices, width), stereo = stereo)]
pub struct StereoFold {
    outputs: StereoFoldOutputs,
    params: StereoFoldParams,
    lane_state: Box<[SwapGlide]>,
}

impl StereoFold {
    fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        let voices = voices(p);
        let input_lanes = p.input.lanes();
        // Note `n` side `c` is lane `n * 2 + c`.
        let notes = self.lane_count() / 2;
        for (note, glide) in self.lane_state.iter_mut().enumerate().take(notes) {
            // A stereo width control sets each note from its sides' average.
            let width = p.width.folded_or(note, StereoFoldParams::WIDTH_DEFAULT);
            let width = glide.follow(
                width.clamp(0.0, 5.0),
                p.width.glide_mark(note * 2),
                sample_rate,
            ) / 5.0;
            // The copies past the input's last lane are those `$unison` cut,
            // the top of the detune range: the rest spread as a smaller stack.
            let present = (0..voices)
                .take_while(|&v| unison_lane(v, note, notes) < input_lanes)
                .count();
            let mut sum = [0.0f32; 2];
            for slot in 0..present {
                let lane = unison_lane(detune_of_slot(slot, present), note, notes);
                let pair = [p.input.get_value(lane, 0), p.input.get_value(lane, 1)];
                let [left, right] = pan_pair(pair, centred_position(slot, present) * width);
                sum[0] += left;
                sum[1] += right;
            }
            let gain = unison_gain(present);
            self.outputs.sample.set(note * 2, sanitize(sum[0] * gain));
            self.outputs
                .sample
                .set(note * 2 + 1, sanitize(sum[1] * gain));
        }
    }
}

message_handlers!(impl StereoFold {});
