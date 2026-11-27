//! `$nam`: runs a signal through a Neural Amp Modeler capture, using the
//! reference NeuralAmpModelerCore runtime.
//!
//! Models are built and prewarmed on the main thread in `prepare_resources`,
//! one per lane, and handed to the lanes in `on_patch_update` only when the
//! file changed; otherwise a lane keeps the model it runs, with its history,
//! across patch edits.

mod model;

use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::block_input::{InputScratch, OptionBlockInput};
use crate::dsp::utils::sanitize;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::reference_category::ReferenceCategory;
use crate::types::{GlideMark, Resources, SeedSide, SwapGlide};

use model::{Built, LaneModel};
pub use model::{NamModel, NamModelData};

use model::MAX_FRAMES;

/// Which version of a capture to run, for captures that ship more than one.
#[derive(Clone, Copy, Debug, Default, Deserr, JsonSchema, PartialEq, Eq, Connect)]
#[serde(rename_all = "snake_case")]
#[deserr(rename_all = lowercase)]
pub enum NamQuality {
    /// the capture as its author made it: very CPU intensive, but may sound better with some captures
    Full,
    /// a lighter version of the capture, for a fraction of the CPU
    #[default]
    Lite,
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct NamParams {
    #[input("signal to run through the capture; 5 V reaches the capture's full scale", type = audio)]
    input: PolySignal,
    #[input("the `.nam` capture to play through, from the workspace root")]
    model: NamModel,
    #[input(
        "how hard the signal drives the capture, in dB",
        type = control,
        default = 0.0,
        range = (-24.0, 24.0),
    )]
    #[deserr(default)]
    gain: Option<PolySignal>,
    #[input(
        "output level in dB; 0 makes every capture about equally loud",
        type = control,
        default = 0.0,
        range = (-24.0, 24.0),
    )]
    #[deserr(default)]
    level: Option<PolySignal>,
    #[input(
        "which version to run of a capture that ships a lite one beside it; `full` costs several times the CPU but may sound better with some captures, and other captures ignore it",
        default = "lite"
    )]
    #[serde(default)]
    #[deserr(default)]
    quality: Option<NamQuality>,
    /// One model per lane, built by `prepare_resources`.
    #[serde(skip)]
    #[deserr(skip)]
    #[schemars(skip)]
    built: Option<Built>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct NamOutputs {
    #[output("output", "the signal through the capture", type = cv, default)]
    sample: PolyOutput,
}

/// A dB value converted to a gain, recomputed only when the dB changes.
#[derive(Clone, Copy)]
struct Decibels {
    db: f32,
    gain: f32,
}

impl Default for Decibels {
    fn default() -> Self {
        Self { db: 0.0, gain: 1.0 }
    }
}

impl Decibels {
    #[inline]
    fn gain(&mut self, db: f32) -> f32 {
        let db = db.clamp(-24.0, 24.0);
        if db != self.db {
            self.db = db;
            self.gain = 10f32.powf(db / 20.0);
        }
        self.gain
    }
}

#[derive(Default)]
struct LaneState {
    model: Option<LaneModel>,
    /// The [`Built::source`] `model` was built from; 0 while it has none.
    source: u64,
    gain: SwapGlide,
    gain_db: Decibels,
    level: SwapGlide,
    level_db: Decibels,
}

/// A side a lane gains starts on its left side's levels; its model arrives
/// in `on_patch_update`, since one model's history cannot be shared.
impl SeedSide for LaneState {
    fn seed_side(&mut self, left: &Self) {
        let Self {
            model: _,
            source: _,
            gain,
            gain_db,
            level,
            level_db,
        } = self;
        *gain = left.gain;
        *gain_db = left.gain_db;
        *level = left.level;
        *level_db = left.level_db;
    }
}

impl LaneState {
    /// The model's input for one sample of `input` volts at `gain` dB.
    #[inline]
    fn drive(&mut self, input: f32, gain: f32, mark: GlideMark, sample_rate: f32) -> f32 {
        let db = self.gain.follow(gain, mark, sample_rate);
        input / 5.0 * self.gain_db.gain(db)
    }

    /// Volts out for one sample of model output at `level` dB.
    #[inline]
    fn output(&mut self, y: f32, level: f32, mark: GlideMark, sample_rate: f32) -> f32 {
        let db = self.level.follow(level, mark, sample_rate);
        let scale = self.model.as_ref().map_or(0.0, |model| model.out_scale);
        sanitize(y * scale * self.level_db.gain(db))
    }

    /// Runs the lane's model, or writes silence when it holds none from
    /// `source`.
    #[inline]
    fn run(&mut self, source: u64, input: &[f32], output: &mut [f32]) {
        let ran = match &mut self.model {
            Some(model) if self.source == source => model.dsp.process(input, output),
            _ => false,
        };
        if !ran {
            output.fill(0.0);
        }
    }
}

#[derive(Default)]
struct NamState {
    /// The [`Built::source`] every lane should be running, or 0 when the file
    /// `model` names did not load.
    source: u64,
}

const NAM_DOCS: ModuleDocs = ModuleDocs {
    summary: "Neural amp capture",
    description: "Plays a signal through a [Neural Amp Modeler](https://www.neuralampmodeler.com) \
                  capture of an amp, pedal or preamp, loaded from a `.nam` file in the workspace. \
                  Captures are trained at 48 kHz and sound as intended only at that rate.",
    categories: &[ReferenceCategory::Distortion],
    sections: &[],
    examples: &[
        DocExample {
            title: "Saw bass through an amp",
            code: r#"
                bass: $saw('e1').nam('/captures/amp.nam', { gain: 6 })
            "#,
        },
        DocExample {
            title: "Chords through the full capture",
            code: r#"
                keys: $saw('c3:m7').nam('/captures/amp.nam', {
                  quality: 'full',
                })
            "#,
        },
    ],
};

#[module(
    name = "$nam",
    docs = NAM_DOCS,
    args(input, model),
    has_prepare_resources,
    patch_update,
    block,
    experimental
)]
pub struct Nam {
    outputs: NamOutputs,
    state: NamState,
    lane_state: Box<[LaneState]>,
    params: NamParams,
}

impl Nam {
    /// Builds and prewarms one model per lane. Main thread.
    fn prepare_resources_impl(&mut self, resources: Resources<'_>, sample_rate: f32) {
        let lanes = self.lane_state.len();
        let quality = self.params.quality.unwrap_or_default();
        self.params.built = resources
            .nam_data
            .get(self.params.model.path())
            .map(|data| Built::new(data, quality, sample_rate, lanes));
    }

    fn update(&mut self, sample_rate: f32) {
        let source = self.state.source;
        for lane in 0..self.lane_count() {
            let state = &mut self.lane_state[lane];
            let x = state.drive(
                self.params.input.get_value(lane),
                self.params.gain.value_or(lane, 0.0),
                self.params.gain.glide_mark(lane),
                sample_rate,
            );
            let mut y = 0.0;
            state.run(source, &[x], std::slice::from_mut(&mut y));
            let out = state.output(
                y,
                self.params.level.value_or(lane, 0.0),
                self.params.level.glide_mark(lane),
                sample_rate,
            );
            self.outputs.sample.set(lane, out);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut NamBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let (mut s0, mut s1, mut s2) = (
            InputScratch::new(),
            InputScratch::new(),
            InputScratch::new(),
        );
        let params = &self.params;
        let input = params.input.block_input(range.clone(), lanes, &mut s0)?;
        let gain = params
            .gain
            .block_input_or(range.clone(), lanes, 0.0, &mut s1)?;
        let level = params
            .level
            .block_input_or(range.clone(), lanes, 0.0, &mut s2)?;
        let source = self.state.source;
        let mut x = [0.0f32; MAX_FRAMES];
        let mut y = [0.0f32; MAX_FRAMES];
        for (lane, state) in self.lane_state[..lanes].iter_mut().enumerate() {
            let (gain_mark, level_mark) =
                (params.gain.glide_mark(lane), params.level.glide_mark(lane));
            let mut start = range.start;
            while start < range.end {
                let end = (start + MAX_FRAMES).min(range.end);
                let n = end - start;
                for (k, slot) in (start..end).enumerate() {
                    x[k] = state.drive(
                        input.row(slot)[lane],
                        gain.row(slot)[lane],
                        gain_mark,
                        sample_rate,
                    );
                }
                state.run(source, &x[..n], &mut y[..n]);
                for (k, slot) in (start..end).enumerate() {
                    outputs.sample.slot_mut(slot)[lane] =
                        state.output(y[k], level.row(slot)[lane], level_mark, sample_rate);
                }
                start = end;
            }
        }
        Some(())
    }
}

impl crate::types::PatchUpdateHandler for Nam {
    /// Hands each lane its freshly built model unless it already runs one
    /// from the same file. The model it gives up drops with the params on
    /// the main thread.
    fn on_patch_update(&mut self) {
        let Some(built) = &mut self.params.built else {
            self.state.source = 0;
            return;
        };
        self.state.source = built.source;
        for (i, lane) in self.lane_state.iter_mut().enumerate() {
            if lane.source != built.source && built.hand_over(i, &mut lane.model) {
                lane.source = built.source;
            }
        }
    }
}

message_handlers!(impl Nam {});
