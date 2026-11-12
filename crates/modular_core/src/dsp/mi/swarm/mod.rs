mod voice;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            limiter::{Limiter, LimiterRates},
            phase::{limit_frequency, max_frequency},
            random::Random,
            units::semitones_to_ratio,
        },
        oscillators::{FmMode, apply_fm},
        utils::{
            SchmittTrigger,
            rng::{hash_bipolar, seed_base},
        },
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
};

use voice::SwarmVoice;

/// Voices in the swarm.
const VOICES: usize = 8;

/// Plaits' saw gain into its limiter, and its sine gain, at ±5 V full scale.
const SAW_PRE_GAIN: f32 = 3.0;
const OUTPUT_GAIN: f32 = -5.0;
const SINE_GAIN: f32 = -5.0;

/// How often the grain envelopes step: Plaits' block rate.
const TICK_RATE: f32 = 4000.0;
/// Plaits' block, in samples at 48 kHz, in which its densities are given.
const PLAITS_BLOCK: f32 = 12.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SwarmParams {
    #[input("the swarm's centre pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "starts a burst: every voice restarts at once with a flurry of quick grains that slows down; unpatched, the swarm runs continuously",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "how often each voice starts a new grain or glide, from rarely (0) to a dense flutter (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    density: Option<PolySignal>,
    #[input(
        "how far the eight voices stray from the centre pitch, from unison (0) to a wide, clustered cloud (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    spread: Option<PolySignal>,
    #[input(
        "from short, separate grains (0) to long grains, then, past about 3.5, continuous glides between pitches (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    grain_size: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs the voices backward",
        type = audio,
    )]
    #[deserr(default)]
    fm: Option<PolySignal>,
    #[input("how `fm` bends the frequency")]
    #[serde(default)]
    #[deserr(default)]
    fm_mode: FmMode,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SwarmOutputs {
    #[output("saw", "the swarm of saws", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
    #[output(
        "sine",
        "the same voices and grains as sines",
        type = cv,
        range = (-5.0, 5.0),
    )]
    sine: PolyOutput,
}

const SWARM_DOCS: ModuleDocs = ModuleDocs {
    summary: "Swarm of grains and glides",
    description: "Mutable Instruments Plaits' swarm: eight saw (or sine) \
                  voices that each wander in pitch, from a cloud of short \
                  grains to a mass of slow glissandi around the pitch.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Cloud of grains",
            code: "$mi.swarm('c3', { spread: 2, grainSize: 1 }).out()",
        },
        DocExample {
            title: "Bursts",
            code: r#"
                const seq = $cycle($p('c3 [~ c3] eb3 <g3 bb2>'))
                $mi.swarm(seq, { trigger: seq._trig })._sine.out()
            "#,
        },
    ],
};

#[module(name = "$mi.swarm", docs = SWARM_DOCS, args(freq), has_init, patch_update)]
pub struct Swarm {
    outputs: SwarmOutputs,
    state: SwarmState,
    channel_state: Box<[Channel]>,
    params: SwarmParams,
}

#[derive(Default)]
struct SwarmState {
    inv_sample_rate: f32,
    to_48k: f32,
    /// Samples per tick: 12 at 48 kHz, below 1 under 4 kHz, where several
    /// ticks share a sample.
    period: f32,
    /// Plaits' 12 kHz saw limit, or Nyquist when that is lower.
    max_frequency: f32,
    limiter: LimiterRates,
}

impl SwarmState {
    fn new(sample_rate: f32) -> Self {
        Self {
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            period: sample_rate / TICK_RATE,
            max_frequency: max_frequency(sample_rate).min(0.5),
            limiter: LimiterRates::new(sample_rate),
        }
    }
}

/// One channel's swarm.
#[derive(Clone, Copy)]
struct Channel {
    voices: [SwarmVoice; VOICES],
    random: Random,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// channel, so a patch update never reseeds a running swarm.
    seeded: bool,
    trigger: SchmittTrigger,
    limiter: Limiter,
    /// Samples until the next tick, and whether a tick has run yet.
    countdown: f32,
    started: bool,
    /// The knobs summed since the last tick, and how many samples that is.
    sum: [f32; 3],
    count: f32,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 3],
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            voices: [SwarmVoice::default(); VOICES],
            random: Random::new(0),
            seeded: false,
            trigger: SchmittTrigger::default(),
            limiter: Limiter::default(),
            countdown: 0.0,
            started: false,
            sum: [0.0; 3],
            count: 0.0,
            held: [
                SwarmParams::DENSITY_DEFAULT,
                SwarmParams::SPREAD_DEFAULT,
                SwarmParams::GRAIN_SIZE_DEFAULT,
            ],
        }
    }
}

fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

/// Plaits' `NoteToFrequency`, here only a rate: cycles per 48 kHz sample at
/// Plaits' corrected codec rate.
fn note_to_frequency(note: f32) -> f32 {
    const A0: f32 = (440.0 / 8.0) / 47872.34;
    A0 * 0.25 * semitones_to_ratio((note - 9.0).clamp(-128.0, 127.0))
}

impl Channel {
    /// Reads a sample's density, spread and grain size and, when a tick is
    /// due or a burst starts, steps every voice's grain from their mean since
    /// the last tick.
    #[inline(always)]
    fn control(
        &mut self,
        knobs: [f32; 3],
        burst_mode: bool,
        start_burst: bool,
        state: &SwarmState,
    ) {
        for ((held, sum), value) in self.held.iter_mut().zip(&mut self.sum).zip(knobs) {
            if value.is_finite() {
                *held = value;
            }
            *sum += *held;
        }
        self.count += 1.0;
        if self.countdown <= 0.0 || start_burst {
            // A burst restarts the timebase on its own sample.
            if start_burst {
                self.countdown = 0.0;
            }
            let [timbre, harmonics, morph] = self.sum.map(|sum| knob(sum / self.count));
            self.sum = [0.0; 3];
            self.count = 0.0;
            let density = note_to_frequency(timbre * 120.0) * 0.025 * PLAITS_BLOCK;
            let spread = harmonics * harmonics * harmonics;
            let size_ratio = 0.25 * semitones_to_ratio((1.0 - morph) * 84.0);
            let mut burst = start_burst;
            while self.countdown <= 0.0 {
                self.countdown += state.period;
                self.tick(density, spread, size_ratio, burst_mode, burst, state);
                burst = false;
            }
        }
        self.countdown -= 1.0;
    }

    /// One step of every voice's grain, ramping each voice's pitch ratio and
    /// level to the result across the tick.
    fn tick(
        &mut self,
        density: f32,
        spread: f32,
        mut size_ratio: f32,
        burst_mode: bool,
        start_burst: bool,
        state: &SwarmState,
    ) {
        let jump = !self.started;
        self.started = true;
        let length = state.period.max(1.0);
        for (i, voice) in self.voices.iter_mut().enumerate() {
            let rank = (i as f32 - 3.0) / 3.0;
            voice
                .envelope
                .step(density, burst_mode, start_burst, &mut self.random);
            let (ratio, level) = voice.targets(rank, spread, size_ratio);
            voice.ratio.set(ratio, length, jump);
            voice.level.set(level, length, false);
            size_ratio *= 0.97;
        }
    }

    /// One sample of the saws and sines at `f0` cycles per sample.
    #[inline(always)]
    fn step(&mut self, f0: f32, state: &SwarmState) -> (f32, f32) {
        let mut saws = 0.0;
        let mut sines = 0.0;
        for voice in &mut self.voices {
            let ratio = voice.ratio.next();
            let level = voice.level.next();
            let f = f0 * ratio;
            saws += voice.saw.step(limit_frequency(f, state.max_frequency)) * level;
            // Plaits' sine fades toward its 12 kHz limit, and toward this
            // rate's when that is lower.
            let speed = f.abs().max(f.abs() * state.to_48k);
            let amplitude = if speed >= 0.25 {
                0.0
            } else {
                level * (1.0 - speed * 4.0)
            };
            sines += voice.sine.render::<1>(f.clamp(-0.25, 0.25), amplitude)[0].0;
        }
        (
            self.limiter.process(saws * SAW_PRE_GAIN, &state.limiter),
            sines,
        )
    }
}

impl Swarm {
    fn init(&mut self, sample_rate: f32) {
        self.state = SwarmState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let burst_mode = p.trigger.is_some();
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, SwarmParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let f0 = limit_frequency(f0, 0.5);
            let start_burst = p
                .trigger
                .as_ref()
                .is_some_and(|t| channel.trigger.process(t.get_value(ch)));
            let knobs = [
                p.density.value_or(ch, SwarmParams::DENSITY_DEFAULT),
                p.spread.value_or(ch, SwarmParams::SPREAD_DEFAULT),
                p.grain_size.value_or(ch, SwarmParams::GRAIN_SIZE_DEFAULT),
            ];
            channel.control(knobs, burst_mode, start_burst, &self.state);
            let (saws, sines) = channel.step(f0, &self.state);
            // Plaits clips after its slow limiter.
            self.outputs
                .sample
                .set(ch, (OUTPUT_GAIN * saws).clamp(-5.0, 5.0));
            self.outputs
                .sine
                .set(ch, (SINE_GAIN * sines).clamp(-5.0, 5.0));
        }
    }
}

impl crate::types::PatchUpdateHandler for Swarm {
    /// Seeds the grain generators of channels that have none yet. Runs here,
    /// not in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same grains.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, channel) in self.channel_state.iter_mut().enumerate() {
            if !channel.seeded {
                let unit = (hash_bipolar(base, ch as u64) + 1.0) * 0.5;
                channel.random = Random::new((unit * f64::from(u32::MAX)) as u32);
                channel.seeded = true;
            }
        }
    }
}

message_handlers!(impl Swarm {});

#[cfg(test)]
mod tests;
