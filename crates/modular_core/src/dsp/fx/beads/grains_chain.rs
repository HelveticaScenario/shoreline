//! The `$granular` chain at the hardware rate of the selected quality: each
//! voice's grain engine and dry/wet mix, then limiters on each stereo lane.
//!
//! Voices come in pairs, one per side of a stereo lane (`l * 2 + c`), each
//! recording its own side. The pair's grains start together with one pan, so
//! summing the pair's placed grains plays each grain as a stereo image.

use super::chain::{BlockClock, Limiters, Media};
use super::granular::GrainStartRequest;
use super::player::{GrainTrigger, bounded_random};
use super::primitives::Frame;
use super::resample::MonoHistory;
use super::voice::{Voice, VoiceControls};
use crate::dsp::utils::rng::LcgRng;

/// Where each side's voice sits in the stereo image: its own side only.
const SIDE_PLACE: [[f32; 2]; 2] = [[1.0, 0.0], [0.0, 1.0]];

#[derive(Default)]
pub struct GrainsVoice {
    pub voice: Voice,
    /// The lane's limiters, kept on its left side's voice.
    limiters: Limiters,
    /// This side of the lane's output at the hardware rate, read at the
    /// engine rate through the resampler.
    pub output: MonoHistory,
    /// Second output of the last pair, emitted on the next hardware sample
    /// (clean tape).
    pair_output: f32,
}

impl GrainsVoice {
    /// Allocate buffers. Main thread only.
    pub fn allocate(&mut self) {
        self.voice.allocate();
        self.output.allocate();
    }

    /// Allocate a voice that plays a buffer handed to it. Main thread only.
    pub fn allocate_playback(&mut self) {
        self.voice.allocate_playback();
        self.output.allocate();
    }

    /// Switch to `media`, clearing the voice's recording, grains and output.
    pub fn configure(&mut self, media: Media) {
        self.voice.configure(media);
        self.limiters.init(media.sample_rate());
        self.output.clear();
        self.pair_output = 0.0;
    }
}

#[derive(Default)]
pub struct GrainsChain {
    media: Media,
    clock: BlockClock,
    /// Position within the current pair of hardware samples (clean tape).
    pair_pos: usize,
}

impl GrainsChain {
    /// Allocate buffers. Main thread only.
    pub fn allocate(&mut self) {
        super::primitives::warm_tables();
    }

    /// Switch every voice to `media`, clearing the recordings and grains.
    pub fn configure(&mut self, media: Media, voices: &mut [GrainsVoice]) {
        self.media = media;
        self.clock.configure(media.sample_rate());
        for voice in voices.iter_mut() {
            voice.configure(media);
        }
        self.pair_pos = 0;
    }

    /// Queue a grain start for its voice; false when that voice already has
    /// one waiting. A trigger for a voice the module no longer has, carried
    /// over from a wider module, is dropped.
    pub fn offer(&mut self, voices: &mut [GrainsVoice], t: GrainTrigger) -> bool {
        let Some(v) = voices.get_mut(t.voice) else {
            return true;
        };
        let voice = &mut v.voice;
        if voice.trigger.is_some() {
            return false;
        }
        let fractional_sample = if self.media.granular_ratio() == 2 {
            (t.fractional_sample + self.pair_pos as f32) * 0.5
        } else {
            t.fractional_sample
        };
        voice.trigger = Some(GrainTrigger {
            fractional_sample,
            ..t
        });
        true
    }

    /// Advance the control-block clock; at a block start, read and smooth
    /// every voice's controls and set its ramps.
    fn begin_sample(
        &mut self,
        voices: &mut [GrainsVoice],
        controls: &mut impl FnMut(usize) -> VoiceControls,
    ) {
        if !self.clock.tick() {
            return;
        }
        let block = self.clock.size();
        let grain_block = block / self.media.granular_ratio();
        for (i, v) in voices.iter_mut().enumerate() {
            v.voice.start_block(controls(i), block, grain_block);
            v.voice.start_place(SIDE_PLACE[i % 2], block);
        }
    }

    /// Resolve each voice's waiting grain start. A voice's grain levels and
    /// stereo spread follow the number of its own grains sounding, and the
    /// two sides of a lane draw one pan for grains they start together.
    fn start_grains(voices: &mut [GrainsVoice], rng: &mut LcgRng) {
        for lane in voices.chunks_mut(2) {
            let mut pan = None;
            for v in lane {
                let voice = &mut v.voice;
                if let Some(trigger) = voice.trigger.take() {
                    let active = (voice.granular.player.active_count() + 1) as f32;
                    let pan = *pan.get_or_insert_with(|| {
                        bounded_random(rng, 0.5 * active * trigger.params.spread)
                    });
                    voice.start = Some(GrainStartRequest {
                        trigger,
                        gain: 1.0 / active.sqrt(),
                        pan,
                    });
                }
            }
        }
    }

    /// Run every voice's grain engine for one grain-engine sample.
    fn render(voices: &mut [GrainsVoice], clean_tape: bool) {
        for v in voices.iter_mut() {
            let voice = &mut v.voice;
            let place = voice.place();
            let start = voice.start.take();
            if clean_tape {
                let low = voice.decimator.output();
                let (wet, _) = voice.granular.process(low, voice.pair_freeze, start, place);
                let l = voice.interpolator[0].process(wet.l);
                let r = voice.interpolator[1].process(wet.r);
                voice.pair_wet = [Frame::new(l[0], r[0]), Frame::new(l[1], r[1])];
                voice.pair_freeze = false;
            } else {
                let (wet, _) = voice
                    .granular
                    .process(voice.input, voice.freeze, start, place);
                voice.pair_input[0] = voice.input;
                voice.pair_wet[0] = wet;
            }
        }
    }

    /// Mix hardware sample `k` of the current pair: each side's dry signal and
    /// both sides' grains, through the lane's limiters, handed to `emit` with
    /// each side's voice.
    fn finish_sample(
        &mut self,
        voices: &mut [GrainsVoice],
        k: usize,
        mut emit: impl FnMut(&mut GrainsVoice, f32),
    ) {
        let media = self.media;
        for lane in voices.chunks_mut(2) {
            let mut out = Frame::ZERO;
            for (side, v) in lane.iter_mut().enumerate() {
                let voice = &mut v.voice;
                // The ramps advance every sample; a voice's place is its own
                // side, so its dry signal goes to that side alone.
                voice.place[0].next();
                voice.place[1].next();
                let d = media.colour(voice.pair_input[k]) * voice.dry.next();
                out += voice.pair_wet[k] * voice.wet.next();
                if side == 0 {
                    out.l += d;
                } else {
                    out.r += d;
                }
            }
            let [left, rest @ ..] = lane else {
                continue;
            };
            let out = left.limiters.pre.process(out);
            let out = left.limiters.output.process(out);
            emit(left, out.l);
            if let [right] = rest {
                emit(right, out.r);
            }
        }
    }

    /// Process one sample at the hardware rate from each voice's `input`,
    /// `freeze` and waiting grain start, writing each side's output to its
    /// voice's `output` history at `index`. In clean tape the hardware
    /// handles each pair of samples together, so the output is one sample
    /// behind the hardware's. `controls` is read once per control block.
    pub fn process(
        &mut self,
        voices: &mut [GrainsVoice],
        controls: &mut impl FnMut(usize) -> VoiceControls,
        rng: &mut LcgRng,
        index: u64,
    ) {
        if self.media.granular_ratio() == 1 {
            self.begin_sample(voices, controls);
            Self::start_grains(voices, rng);
            Self::render(voices, false);
            self.finish_sample(voices, 0, |v, x| v.output.write(index, x));
            return;
        }

        // Clean tape: the grain engine runs at half rate between the
        // hardware's 2× decimation and interpolation filters.
        for v in voices.iter_mut() {
            let voice = &mut v.voice;
            voice.pair_input[self.pair_pos] = voice.input;
            voice.pair_freeze |= voice.freeze;
        }
        if self.pair_pos == 0 {
            self.pair_pos = 1;
            for v in voices.iter_mut() {
                v.output.write(index, v.pair_output);
            }
            return;
        }
        self.pair_pos = 0;

        for v in voices.iter_mut() {
            let voice = &mut v.voice;
            voice.decimator.push(voice.pair_input[0]);
            voice.decimator.push(voice.pair_input[1]);
        }
        self.begin_sample(voices, controls);
        Self::start_grains(voices, rng);
        Self::render(voices, true);

        self.finish_sample(voices, 0, |v, x| v.output.write(index, x));
        self.begin_sample(voices, controls);
        self.finish_sample(voices, 1, |v, x| v.pair_output = x);
    }
}
