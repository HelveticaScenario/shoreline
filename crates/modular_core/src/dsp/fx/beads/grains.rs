//! `$granular`: Beads' grain voices on a live input, with a stereo mix and
//! one output channel per voice.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::rng::{LcgRng, seed_base};
use crate::poly::{MonoSignal, MonoSignalExt, PolyOutput, PolySignal, PolySignalExt};
use crate::types::PatchUpdateHandler;

use super::chain::Media;
use super::fresh::FreshBuffers;
use super::grains_chain::{GrainsChain, GrainsVoice};
use super::player::{GrainParams, GrainTrigger};
use super::queue::TriggerQueue;
use super::resample::Resampler;
use super::voice::VoiceControls;
use super::wav_load::silent_buffer;
use super::{FULL_SCALE_VOLTS, GranularQuality, grain_params, input_value, unit};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct GranularParams {
    #[input(
        "signal to record; each channel gets its own voice",
        type = audio,
    )]
    input: PolySignal,
    #[input("starts a grain", type = trig)]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "while high, recording stops and grains keep playing the audio already held",
        type = gate,
    )]
    #[deserr(default)]
    freeze: Option<PolySignal>,
    #[input(
        "where grains start: 0 the most recent audio, 5 the oldest",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    time: Option<PolySignal>,
    #[input(
        "grain length: 0 shortest, 5 longest; negative values play the grains backwards",
        type = control,
        default = 2.75,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    size: Option<PolySignal>,
    #[input(
        "grain envelope: 0 square, 1.67 sharp attack, 3.33 smooth, 5 reverse swell",
        type = control,
        default = 3.33,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    shape: Option<PolySignal>,
    #[input(
        "grain pitch; grains keep the input's pitch when `freq` equals `rootFreq`",
        type = pitch,
    )]
    #[deserr(default)]
    freq: Option<PolySignal>,
    #[input("pitch of the input, so `freq` plays in tune", type = pitch)]
    #[deserr(default)]
    root_freq: Option<PolySignal>,
    #[input(
        "how far each grain lands at a random stereo position: 0 centred, 5 anywhere",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    spread: Option<PolySignal>,
    #[input(
        "spreads the voices across the stereo field: 0 all centred, 5 first voice hard left to last hard right",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    width: Option<MonoSignal>,
    #[input(
        "feeds the grains back into the recording, building dense washes",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    feedback: Option<PolySignal>,
    #[input(
        "dry/wet blend: 0 dry, 5 grains only",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    mix: Option<PolySignal>,
    #[input(
        "recording character: `'digital'` holds 4 s of audio, `'vintage'` 8 s, `'cleanTape'` 10.7 s, `'dirtyTape'` 16 s",
        default = "digital"
    )]
    #[serde(default)]
    #[deserr(default)]
    quality: GranularQuality,
    /// A silent recording buffer per voice at `quality`, made by `init`.
    #[serde(skip)]
    #[deserr(skip)]
    #[schemars(skip)]
    fresh: Option<FreshBuffers>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GranularOutputs {
    #[output(
        "output",
        "stereo mix of every voice, as left and right channels",
        default,
        range = (-5.0, 5.0),
        channels = 2
    )]
    sample: PolyOutput,
    #[output("voices", "each voice on its own channel, mono", range = (-5.0, 5.0))]
    voices: PolyOutput,
}

#[derive(Default)]
struct GranularState {
    engine_sample_rate: f32,
    media: Media,
    chain: GrainsChain,
    resampler: Resampler,
    rng: LcgRng,
    /// Engine samples processed.
    sample: u64,
    pending: TriggerQueue,
}

const GRANULAR_DOCS: ModuleDocs = ModuleDocs {
    summary: "Live granular processor",
    description: "The granular processor from Mutable Instruments Beads: it records its input \
                  and plays it back as grains, one per trigger, for clouds, smears and stutters \
                  of live sound. For grain-to-grain variation, modulate its settings with \
                  something sampled on the same trigger, such as `$sah`. Follow it with `$wowFlutter` and `$reverb` for the \
                  rest of Beads' sound.",
    categories: &[ModuleCategory::Effect],
    sections: &[],
    examples: &[DocExample {
        title: "Chord cloud from a single saw",
        code: r#"
            const grains = $pulse('8hz')
            $granular($saw('a3'), grains, {
              time: $sah($noise('white').range(0, 2), grains),
              freq: $cycle($p('[c4, e4, g4] [b3, d4, g4]')),
              rootFreq: 'a3',
              width: 4,
              spread: 2,
              mix: 4,
            }).out()
        "#,
    }],
};

#[module(
    name = "$granular",
    docs = GRANULAR_DOCS,
    has_init,
    patch_update,
    args(input, trigger)
)]
pub struct Granular {
    outputs: GranularOutputs,
    state: GranularState,
    params: GranularParams,
    /// One voice per channel of the widest input.
    channel_state: Box<[GrainsVoice]>,
}

impl Granular {
    /// Allocate every voice and a fresh recording buffer for each, and
    /// configure for the initial quality. Until `on_patch_update` hands them
    /// their buffers, the voices share a silent one. Main thread.
    fn init(&mut self, sample_rate: f32) {
        let seed = seed_base(self);
        self.state.chain.allocate();
        let media = self.params.quality.media();
        let silence = silent_buffer(media);
        for voice in self.channel_state.iter_mut() {
            voice.allocate();
            voice.voice.granular.buffer = silence.clone();
        }
        self.params.fresh = Some(FreshBuffers::allocate(self.channel_state.len(), media));
        let state = &mut self.state;
        state.engine_sample_rate = sample_rate;
        state.resampler.allocate(sample_rate);
        state.rng.seed(seed, 0);
        self.configure(media);
    }

    fn configure(&mut self, media: Media) {
        let state = &mut self.state;
        state.media = media;
        state.chain.configure(media, &mut self.channel_state);
        state
            .resampler
            .configure(state.engine_sample_rate, media.sample_rate());
        state.pending.clear();
    }

    fn voice_controls(params: &GranularParams, voice: usize) -> VoiceControls {
        VoiceControls {
            feedback: unit(params.feedback.value_or(voice, 0.0)),
            dry_wet: unit(params.mix.value_or(voice, 2.5)),
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let params = &self.params;
        let state = &mut self.state;
        let voices = &mut self.channel_state;

        let index = state.resampler.input_index();
        let converts = state.resampler.converts();
        for (i, v) in voices.iter_mut().enumerate() {
            let onset = v.voice.sense(
                input_value(&params.input, i),
                converts,
                index,
                params.trigger.as_ref().map(|t| t.get_value(i)),
                params.freeze.as_ref().map(|f| f.get_value(i)),
            );
            if let Some(age) = onset {
                let p = params;
                let grain = GrainTrigger {
                    voice: i,
                    fractional_sample: 0.0,
                    params: GrainParams {
                        spread: unit(p.spread.value_or(i, 0.0)),
                        ..grain_params(&p.time, &p.size, &p.shape, &p.freq, &p.root_freq, i)
                    },
                };
                state.pending.push(state.sample, age, grain);
            }
        }

        let GranularState {
            chain,
            resampler,
            rng,
            sample,
            pending,
            ..
        } = state;
        let now = *sample;
        *sample += 1;
        let delay = resampler.input_delay();
        let ratio = resampler.ratio();
        let mut controls = |i: usize| Self::voice_controls(params, i);
        let mut width = || params.width.value_or(0.0).clamp(0.0, 5.0);

        let tap = resampler.tick(|inner, out_index| {
            if converts {
                for v in voices.iter_mut() {
                    v.voice.input = inner.read(&v.voice.history);
                }
            }
            if !pending.is_empty() {
                pending.take_due(now, inner.ago, delay, ratio, |t| chain.offer(voices, t));
            }
            chain.process(voices, &mut controls, &mut width, rng, out_index);
        });

        for (i, v) in voices.iter().enumerate() {
            self.outputs
                .voices
                .set(i, tap.read(&v.output) * FULL_SCALE_VOLTS);
        }
        for (ch, h) in chain.stereo.iter().enumerate() {
            self.outputs.sample.set(ch, tap.read(h) * FULL_SCALE_VOLTS);
        }
    }
}

impl PatchUpdateHandler for Granular {
    fn on_patch_update(&mut self) {
        let channels = self.params.input.channels();
        for (i, v) in self.channel_state.iter_mut().enumerate() {
            v.voice.dry_source = i < channels;
        }
        // A quality change clears every voice, and a voice new to this
        // module still holds the silent stand-in: both take a fresh buffer,
        // which needs no clearing here. Other voices keep recording into
        // their own.
        let media = self.params.quality.media();
        let reconfigure = media != self.state.media;
        if let Some(fresh) = &mut self.params.fresh {
            for (i, v) in self.channel_state.iter_mut().enumerate() {
                if fresh.hand_out(i, &mut v.voice.granular.buffer, reconfigure) && !reconfigure {
                    v.configure(media);
                }
            }
        }
        if reconfigure {
            self.configure(media);
        }
    }
}

message_handlers!(impl Granular {});

#[cfg(test)]
#[path = "tests_grains.rs"]
mod tests;
