mod diffuser;
mod particle;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            filter::{Svf, tan_dirty},
            limiter::{Limiter, LimiterRates},
            phase::limit_frequency,
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

use diffuser::{Diffuser, DiffuserRates};
use particle::{MAX_TICK, Particle};

/// Particles per channel.
const PARTICLES: usize = 6;

/// Plaits' gain into its limiter, and its raw-dust gain, at ±5 V full scale.
const PRE_GAIN: f32 = 2.0;
const OUTPUT_GAIN: f32 = -5.0;
const RAW_GAIN: f32 = -5.0;

/// How often the particles draw: Plaits' block rate.
const TICK_RATE: f32 = 4000.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiParticlesParams {
    #[input("the pitch the particles ring around", type = pitch)]
    freq: PolySignal,
    #[input(
        "fires every particle at once; unpatched, they only crackle at random",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "how often particles crackle, from a sparse tick (0) to a dense hiss (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    density: Option<PolySignal>,
    #[input(
        "how far each particle's pitch strays from `freq`, from all on it (0) to four octaves either way (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    spread: Option<PolySignal>,
    #[input(
        "below the middle, smears the particles into a reverberant cloud (0); above it, narrows their filters into ringing, pitched drops (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    resonance: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); the filters take the frequency's size, so throughZero acts as lin",
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
struct MiParticlesOutputs {
    #[output(
        "filtered",
        "the particles ringing through their filters",
        type = cv,
        default,
        range = (-5.0, 5.0)
    )]
    sample: PolyOutput,
    #[output(
        "raw",
        "the random impulses that drive the filters",
        type = cv,
        range = (-5.0, 5.0),
    )]
    raw: PolyOutput,
}

const PARTICLES_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonant dust",
    description: "Mutable Instruments Plaits' particle noise: random \
                  impulses ringing resonant filters tuned around the pitch, \
                  from crackle and rain to dripping water and a pitched, \
                  reverberant cloud.",
    categories: &[ModuleCategory::Oscillator, ModuleCategory::Random],
    sections: &[],
    examples: &[
        DocExample {
            title: "Dripping",
            code: "$mi.particles('c5', { density: 1.5, resonance: 4.5 }).out()",
        },
        DocExample {
            title: "Diffuse cloud",
            code: "$mi.particles('c4', { spread: 4, resonance: 0.5 }).out()",
        },
    ],
};

#[module(name = "$mi.particles", docs = PARTICLES_DOCS, args(freq), has_init, patch_update)]
pub struct MiParticles {
    outputs: MiParticlesOutputs,
    state: ParticlesState,
    channel_state: Box<[Channel]>,
    params: MiParticlesParams,
}

#[derive(Default)]
struct ParticlesState {
    inv_sample_rate: f32,
    to_48k: f32,
    /// Samples per tick: 12 at 48 kHz, below 1 under 4 kHz, where several
    /// ticks share a sample.
    period: f32,
    limiter: LimiterRates,
    diffuser: DiffuserRates,
}

impl ParticlesState {
    fn new(sample_rate: f32) -> Self {
        Self {
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            period: (sample_rate / TICK_RATE).min(MAX_TICK as f32),
            limiter: LimiterRates::new(sample_rate),
            diffuser: DiffuserRates::new(sample_rate),
        }
    }
}

/// One channel's particles.
struct Channel {
    particles: [Particle; PARTICLES],
    random: Random,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// channel, so a patch update never reseeds it.
    seeded: bool,
    trigger: SchmittTrigger,
    post_filter: Svf,
    /// The pitch, as bits, `post_filter` is tuned to.
    post_filter_pitch: Option<u32>,
    diffuser: Diffuser,
    limiter: Limiter,
    /// Samples until the next tick, and this sample's place in the tick.
    countdown: f32,
    position: usize,
    /// This tick's impulse probability per sample at 48 kHz.
    density_48k: f32,
    /// The density knob summed since the last tick, and how many samples
    /// that is.
    sum: f32,
    count: f32,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 3],
}

impl Channel {
    /// A channel with its diffuser sized for `state`'s rate. Allocates.
    fn new(state: &ParticlesState) -> Self {
        Self {
            diffuser: Diffuser::new(&state.diffuser),
            ..Self::default()
        }
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            particles: [Particle::default(); PARTICLES],
            random: Random::new(0),
            seeded: false,
            trigger: SchmittTrigger::default(),
            post_filter: Svf::default(),
            post_filter_pitch: None,
            diffuser: Diffuser::default(),
            limiter: Limiter::default(),
            countdown: 0.0,
            position: 0,
            density_48k: 0.0,
            sum: 0.0,
            count: 0.0,
            held: [
                MiParticlesParams::DENSITY_DEFAULT,
                MiParticlesParams::SPREAD_DEFAULT,
                MiParticlesParams::RESONANCE_DEFAULT,
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
    /// Reads a sample's knobs and, when a tick is due or `sync` fires every
    /// particle, draws the next tick's impulses from the density knob's
    /// mean since the last tick.
    #[inline(always)]
    fn control(&mut self, knobs: [f32; 3], sync: bool, state: &ParticlesState) {
        for (held, value) in self.held.iter_mut().zip(knobs) {
            if value.is_finite() {
                *held = value;
            }
        }
        self.sum += self.held[0];
        self.count += 1.0;
        if self.countdown <= 0.0 || sync {
            if sync {
                self.countdown = 0.0;
            }
            let timbre = knob(self.sum / self.count);
            self.sum = 0.0;
            self.count = 0.0;
            let density_sqrt = note_to_frequency(60.0 + timbre * timbre * 72.0);
            self.density_48k = density_sqrt * density_sqrt * (1.0 / PARTICLES as f32);
            let density = self.density_48k / state.to_48k;
            let mut sync = sync;
            while self.countdown <= 0.0 {
                self.countdown += state.period;
                // The samples this tick covers; none when another tick follows
                // within this sample.
                let samples = (self.countdown.ceil().max(0.0) as usize).min(MAX_TICK);
                for particle in &mut self.particles {
                    particle.draw_tick(samples, density, sync, &mut self.random);
                }
                sync = false;
            }
            self.position = 0;
        }
        self.countdown -= 1.0;
    }

    /// One sample at `f0` cycles per sample (positive): the filtered,
    /// diffused particles and the raw impulses.
    #[inline(always)]
    fn step(&mut self, f0: f32, state: &ParticlesState) -> (f32, f32) {
        let [_, harmonics, morph] = self.held.map(knob);
        let q_sqrt = semitones_to_ratio(if morph >= 0.5 {
            (morph - 0.5) * 120.0
        } else {
            0.0
        });
        let q = 0.5 + q_sqrt * q_sqrt;
        let spread = 48.0 * harmonics * harmonics;
        let raw_diffusion_sqrt = 2.0 * (morph - 0.5).abs();
        let raw_diffusion = raw_diffusion_sqrt * raw_diffusion_sqrt;
        let diffusion = if morph < 0.5 { raw_diffusion } else { 0.0 };

        let to_48k = state.to_48k;
        let density = self.density_48k / to_48k;
        let gain = 1.0 / density;
        let f0_48k = f0 * to_48k;
        let k = self.position.min(MAX_TICK - 1);
        self.position += 1;
        let mut raw = 0.0;
        let mut out = 0.0;
        for particle in &mut self.particles {
            let (s, filtered) = particle.step(
                k,
                density,
                gain,
                f0_48k,
                spread,
                q,
                self.density_48k,
                to_48k,
            );
            raw += s;
            out += filtered;
        }

        if self.post_filter_pitch != Some(f0.to_bits()) {
            self.post_filter_pitch = Some(f0.to_bits());
            self.post_filter.set_g_q(tan_dirty(f0.min(0.49)), 0.5);
        }
        let out = self.post_filter.low_pass(out);
        let out = self.diffuser.process(
            0.8 * diffusion * diffusion,
            0.5 * diffusion + 0.25,
            out,
            &state.diffuser,
        );
        (self.limiter.process(out * PRE_GAIN, &state.limiter), raw)
    }
}

impl MiParticles {
    fn init(&mut self, sample_rate: f32) {
        self.state = ParticlesState::new(sample_rate);
        for channel in self.channel_state.iter_mut() {
            *channel = Channel::new(&self.state);
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, MiParticlesParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            // A resonator has no backward; at a standstill its filters would
            // freeze, so a 1 Hz floor lets them ring out.
            let f0 = limit_frequency(f0.abs(), 0.5).max(self.state.inv_sample_rate);
            let sync = p
                .trigger
                .as_ref()
                .is_some_and(|t| channel.trigger.process(t.get_value(ch)));
            let knobs = [
                p.density.value_or(ch, MiParticlesParams::DENSITY_DEFAULT),
                p.spread.value_or(ch, MiParticlesParams::SPREAD_DEFAULT),
                p.resonance
                    .value_or(ch, MiParticlesParams::RESONANCE_DEFAULT),
            ];
            channel.control(knobs, sync, &self.state);
            let (out, raw) = channel.step(f0, &self.state);
            // Plaits clips after its slow limiter.
            self.outputs
                .sample
                .set(ch, (OUTPUT_GAIN * out).clamp(-5.0, 5.0));
            self.outputs.raw.set(ch, (RAW_GAIN * raw).clamp(-5.0, 5.0));
        }
    }
}

impl crate::types::PatchUpdateHandler for MiParticles {
    /// Seeds the random generators of channels that have none yet. Runs here,
    /// not in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same impulses.
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

message_handlers!(impl MiParticles {});

#[cfg(test)]
mod tests;
