//! One grain voice: its recording buffer and grains, its mix and stereo
//! position, and its trigger and freeze detection.

use super::chain::{Media, POT_LP};
use super::granular::{GrainStartRequest, GranularCore};
use super::player::GrainTrigger;
use super::primitives::{Decimator2, Frame, Interpolator2, Ramp};
use super::resample::MonoHistory;
use crate::dsp::oscillators::sync_edge_fraction;
use crate::dsp::utils::SchmittTrigger;

/// Per-voice controls in hardware units (0 to 1), read once per block.
#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceControls {
    pub feedback: f32,
    pub dry_wet: f32,
}

#[derive(Default)]
pub struct Voice {
    pub granular: GranularCore,
    /// Engine-rate input, read at the hardware rate through the resampler
    /// when the rates differ.
    pub history: MonoHistory,
    /// Hardware-rate input for the current sample.
    pub input: f32,
    pub freeze: bool,
    pub freeze_schmitt: SchmittTrigger,
    pub trigger_schmitt: SchmittTrigger,
    pub trigger_previous: f32,
    /// Grain start waiting for the next grain-engine sample.
    pub trigger: Option<GrainTrigger>,
    pub start: Option<GrainStartRequest>,

    pub smoothed: VoiceControls,
    /// Whether the voice has run a control block. Before its first, its
    /// controls and placement start at their targets instead of gliding in
    /// from 0, so a new voice doesn't leak its dry signal while `mix` rises.
    pub started: bool,
    pub dry: Ramp,
    pub wet: Ramp,
    /// Left and right gains placing the voice in the stereo field.
    pub place: [Ramp; 2],

    /// Clean tape's 2:1 conversion around the grain engine, and the pair of
    /// hardware samples it processes together.
    pub decimator: Decimator2,
    pub interpolator: [Interpolator2; 2],
    pub pair_input: [f32; 2],
    pub pair_freeze: bool,
    pub pair_wet: [Frame; 2],
}

impl Voice {
    /// Allocate the input history. The recording buffer is handed to the
    /// voice separately, so a voice can keep its recording across patch
    /// updates. Main thread only.
    pub fn allocate(&mut self) {
        self.history.allocate();
        self.allocate_playback();
    }

    /// Prepare a voice that plays a buffer handed to it and records nothing,
    /// so it needs neither a recording buffer nor an input history.
    pub fn allocate_playback(&mut self) {
        for p in self.place.iter_mut() {
            p.reset(1.0);
        }
    }

    /// Switch to `media`, clearing the recording, grains and input history.
    pub fn configure(&mut self, media: Media) {
        let ratio = media.granular_ratio();
        self.granular.reset(
            media.sample_rate() / ratio as f32,
            media.resolution(),
            media.max_delay(),
        );
        self.history.clear();
        self.trigger = None;
        self.start = None;
        self.decimator = Decimator2::default();
        self.interpolator = Default::default();
        self.pair_input = [0.0; 2];
        self.pair_freeze = false;
        self.pair_wet = [Frame::ZERO; 2];
    }

    /// Take this engine sample's `input` (hardware units), writing it to the
    /// history at `index` when the resampler `converts`, and its `trigger` and
    /// `freeze` levels. Returns the onset's age in engine samples (0..1] when
    /// a grain trigger arrives.
    pub fn sense(
        &mut self,
        input: f32,
        converts: bool,
        index: u64,
        trigger: Option<f32>,
        freeze: Option<f32>,
    ) -> Option<f32> {
        if converts {
            self.history.write(index, input);
        } else {
            self.input = input;
        }
        self.freeze = match freeze {
            Some(f) => self.freeze_schmitt.process_with_edge(f).0,
            None => false,
        };
        let v = trigger?;
        let onset = self
            .trigger_schmitt
            .process(v)
            .then(|| 1.0 - sync_edge_fraction(self.trigger_previous, v));
        self.trigger_previous = v;
        onset
    }

    /// Smooth this block's `controls` and set the gain ramps over `block`
    /// hardware samples (`grain_block` grain-engine samples).
    pub fn start_block(&mut self, controls: VoiceControls, block: usize, grain_block: usize) {
        if !self.started {
            self.smoothed = controls;
        }
        let s = &mut self.smoothed;
        s.feedback += POT_LP * (controls.feedback - s.feedback);
        s.dry_wet += POT_LP * (controls.dry_wet - s.dry_wet);
        let s = *s;
        self.granular.start_block(s.feedback, grain_block);
        let x = s.dry_wet;
        self.dry.start((1.0 - x) * (1.0 + x) * 0.75, block);
        self.wet.start(x * (2.0 - x) * 1.5, block);
    }

    /// Ramp the voice's stereo placement to `place` over `block` samples.
    /// Called after [`Self::start_block`], it ends the voice's first block.
    pub fn start_place(&mut self, place: [f32; 2], block: usize) {
        for (ramp, &target) in self.place.iter_mut().zip(&place) {
            if self.started {
                ramp.start(target, block);
            } else {
                ramp.reset(target);
            }
        }
        self.started = true;
    }

    /// Become a copy of `other`, writing into the storage this voice holds,
    /// so it carries on where `other` is. Allocation-free.
    pub fn copy_from(&mut self, other: &Voice) {
        let Self {
            granular: _,
            history: _,
            input: _,
            freeze: _,
            freeze_schmitt: _,
            trigger_schmitt: _,
            trigger_previous: _,
            trigger: _,
            start: _,
            smoothed: _,
            started: _,
            dry: _,
            wet: _,
            place: _,
            decimator: _,
            interpolator: _,
            pair_input: _,
            pair_freeze: _,
            pair_wet: _,
        } = other;
        self.granular.copy_from(&other.granular);
        self.history.copy_from(&other.history);
        self.input = other.input;
        self.freeze = other.freeze;
        self.freeze_schmitt = other.freeze_schmitt;
        self.trigger_schmitt = other.trigger_schmitt;
        self.trigger_previous = other.trigger_previous;
        self.trigger = other.trigger;
        self.start = other.start;
        self.smoothed = other.smoothed;
        self.started = other.started;
        self.dry = other.dry;
        self.wet = other.wet;
        self.place = other.place;
        self.decimator = other.decimator;
        self.interpolator = other.interpolator;
        self.pair_input = other.pair_input;
        self.pair_freeze = other.pair_freeze;
        self.pair_wet = other.pair_wet;
    }

    /// Clear what the voice holds of its grains after placing them: the
    /// clean-tape interpolators and the pair's waiting sample. A voice
    /// copied from one placed in the centre starts so, since the original
    /// keeps that history, and the filters being linear, the two add up to
    /// it exactly.
    pub fn clear_placed(&mut self) {
        self.interpolator = Default::default();
        self.pair_wet = [Frame::ZERO; 2];
    }

    /// Ramp the voice's placement to `place` over `samples` samples.
    pub fn glide_place(&mut self, place: [f32; 2], samples: usize) {
        for (ramp, &target) in self.place.iter_mut().zip(&place) {
            ramp.start(target, samples.max(1));
        }
    }

    /// Place the voice at `place` at once.
    pub fn snap_place(&mut self, place: [f32; 2]) {
        for (ramp, &target) in self.place.iter_mut().zip(&place) {
            ramp.reset(target);
        }
    }

    #[inline]
    pub fn place(&self) -> [f32; 2] {
        [self.place[0].value(), self.place[1].value()]
    }
}
