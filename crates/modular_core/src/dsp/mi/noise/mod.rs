mod clocked;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            filter::{Svf, tan_accurate},
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
    types::{GlideMark, SwapGlide},
};

use clocked::ClockedNoise;

/// Plaits' output gain for this engine's `out` and `aux`, after its limiter,
/// at ±5 V full scale.
const OUTPUT_GAIN: f32 = -5.0;

/// The highest filter frequency, in cycles per sample.
const MAX_FILTER: f32 = 0.49;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiNoiseParams {
    #[input("the frequency the filters centre on", type = pitch)]
    freq: PolySignal,
    #[input(
        "restarts the noise clock, so each hit steps to a fresh value; patched, `rate` reaches two octaves lower",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "the noise clock, from slow, stepped crackle (0) to smooth hiss (5)",
        type = control,
        default = 5.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    rate: Option<PolySignal>,
    #[input(
        "sweeps `filtered` from lowpass (0) through bandpass to highpass (5), and moves `bands`' second band from two octaves below `freq` to two above",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    mode: Option<PolySignal>,
    #[input(
        "from a gentle slope (0) to a whistling, pitched peak (5)",
        type = control,
        default = 1.0,
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
struct MiNoiseOutputs {
    #[output(
        "filtered",
        "the noise through one filter that `mode` sweeps",
        type = cv,
        default,
        range = (-5.0, 5.0)
    )]
    sample: PolyOutput,
    #[output(
        "bands",
        "the noise through two bandpass filters, one at `freq` and one that `mode` moves",
        type = cv,
        range = (-5.0, 5.0),
    )]
    bands: PolyOutput,
}

const NOISE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Clocked, filtered noise",
    description: "Mutable Instruments Plaits' filtered noise: white noise, \
                  held between ticks of a clock, through a resonant filter, \
                  for wind, surf and whistles or stepped, digital hiss.",
    categories: &[ModuleCategory::Oscillator, ModuleCategory::Random],
    sections: &[],
    examples: &[
        DocExample {
            title: "Whistling wind",
            code: "wind: $mi.noise('c5', { resonance: 4 })",
        },
        DocExample {
            title: "Two bands",
            code: "air: $mi.noise('c4', { mode: 3.75, resonance: 3 })._bands",
        },
        DocExample {
            title: "Stepped noise",
            code: r#"
                const beat = $clock._beatTrigger
                bits: $mi.noise('c3', { trigger: beat, rate: 1.5 })
            "#,
        },
    ],
};

#[module(name = "$mi.noise", docs = NOISE_DOCS, args(freq), has_init, patch_update)]
pub struct MiNoise {
    outputs: MiNoiseOutputs,
    state: NoiseState,
    lane_state: Box<[Lane]>,
    params: MiNoiseParams,
}

#[derive(Default)]
struct NoiseState {
    inv_sample_rate: f32,
    to_48k: f32,
    /// Scales a white draw to the same level per hertz as at 48 kHz.
    raw_scale: f32,
    limiter: LimiterRates,
}

impl NoiseState {
    fn new(sample_rate: f32) -> Self {
        let to_48k = sample_rate / 48000.0;
        Self {
            inv_sample_rate: 1.0 / sample_rate,
            to_48k,
            raw_scale: to_48k.sqrt(),
            limiter: LimiterRates::new(sample_rate),
        }
    }
}

/// The controls derived from a sample's pitch and knobs.
#[derive(Clone, Copy, Default)]
struct Controls {
    /// Each noise clock in cycles per sample, and how much white noise
    /// replaces its held steps.
    clock: [f32; 2],
    raw_amount: [f32; 2],
    gain: f32,
    mode: f32,
}

#[derive(Clone, Copy)]
struct Lane {
    noise: [ClockedNoise; 2],
    random: Random,
    /// Whether `random` has its per-instance seed. Transfers with the lane,
    /// so a patch update never reseeds it.
    seeded: bool,
    trigger: SchmittTrigger,
    lp_hp: Svf,
    bp: [Svf; 2],
    limiter: [Limiter; 2],
    mode_glide: SwapGlide,
    resonance_glide: SwapGlide,
    controls: Controls,
    /// The pitch, knobs and trigger state, as bits, `controls` and the
    /// filters were set from.
    control_inputs: Option<[u32; 5]>,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 3],
}

impl Default for Lane {
    fn default() -> Self {
        Self {
            noise: [ClockedNoise::default(); 2],
            random: Random::new(0),
            seeded: false,
            trigger: SchmittTrigger::default(),
            lp_hp: Svf::default(),
            bp: [Svf::default(); 2],
            limiter: [Limiter::default(); 2],
            mode_glide: SwapGlide::default(),
            resonance_glide: SwapGlide::default(),
            controls: Controls::default(),
            control_inputs: None,
            held: [
                MiNoiseParams::RATE_DEFAULT,
                MiNoiseParams::MODE_DEFAULT,
                MiNoiseParams::RESONANCE_DEFAULT,
            ],
        }
    }
}

/// A new side keeps its own `random`, so the sides' noise stays independent.
impl crate::types::SeedSide for Lane {
    fn seed_side(&mut self, left: &Self) {
        *self = Self {
            random: self.random,
            seeded: self.seeded,
            ..*left
        };
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

impl Lane {
    /// Retunes the clocks and filters when the pitch, a knob or whether the
    /// trigger is patched has changed. `knobs` are `rate`, `mode` and
    /// `resonance` in volts.
    #[inline(always)]
    fn control(&mut self, f0: f32, knobs: [f32; 3], patched: bool, state: &NoiseState) {
        for (held, value) in self.held.iter_mut().zip(knobs) {
            if value.is_finite() {
                *held = value;
            }
        }
        let inputs = [
            f0.to_bits(),
            self.held[0].to_bits(),
            self.held[1].to_bits(),
            self.held[2].to_bits(),
            u32::from(patched),
        ];
        if self.control_inputs == Some(inputs) {
            return;
        }
        self.control_inputs = Some(inputs);
        let [timbre, harmonics, morph] = self.held.map(knob);
        let f1 = f0 * semitones_to_ratio(harmonics * 48.0 - 24.0);
        let lowest = if patched { -24.0 } else { 0.0 };
        let clock = note_to_frequency(timbre * (128.0 - lowest) + lowest);
        let q = 0.5 * semitones_to_ratio(morph * 120.0);
        let to_48k = state.to_48k;
        let clocks = [clock, clock * f1 / f0];
        let c = &mut self.controls;
        for (i, clock_48k) in clocks.into_iter().enumerate() {
            c.clock[i] = (clock_48k / to_48k).clamp(0.0, 1.0);
            c.raw_amount[i] = (4.0 * (c.clock[i] * to_48k - 0.25)).clamp(0.0, 1.0);
        }
        // Normalised in 48 kHz terms, so the held noise, whose level per
        // hertz follows its clock, sounds alike at any rate.
        c.gain = 1.0 / ((0.5 + q) * 40.0 * (f0 * to_48k)).sqrt();
        c.mode = harmonics;
        let g0 = tan_accurate(f0);
        self.lp_hp.set_g_q(g0, q);
        self.bp[0].set_g_q(g0, q);
        self.bp[1].set_g_q(tan_accurate(f1.min(MAX_FILTER)), q);
    }

    /// One sample at `f0` cycles per sample (1 Hz to `MAX_FILTER`), synced
    /// when `sync` rises: `filtered` and `bands` after the limiters, before
    /// the output gain.
    #[inline(always)]
    fn step(&mut self, sync: bool, state: &NoiseState) -> (f32, f32) {
        let c = self.controls;
        let mut noise = [0.0; 2];
        for (i, n) in noise.iter_mut().enumerate() {
            let white = self.random.float() * 2.0 - 1.0;
            *n = self.noise[i].step(
                sync,
                c.clock[i],
                white,
                white * state.raw_scale,
                c.raw_amount[i],
            ) * c.gain;
        }
        let out = self.lp_hp.low_to_high_pass(noise[0], c.mode);
        let aux = self.bp[0].band_pass(noise[0]) + self.bp[1].band_pass(noise[1]);
        (
            self.limiter[0].process(out, &state.limiter),
            self.limiter[1].process(aux, &state.limiter),
        )
    }
}

impl MiNoise {
    fn init(&mut self, sample_rate: f32) {
        self.state = NoiseState::new(sample_rate);
    }

    fn update(&mut self, sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        let patched = p.trigger.is_some();
        for (lane_index, lane) in self.lane_state[..lanes].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(lane_index),
                p.fm.value_or(lane_index, MiNoiseParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            // A filter has no backward; at a standstill it would freeze and
            // its gain would grow without bound, so a 1 Hz floor keeps it
            // ringing.
            let f0 = limit_frequency(f0.abs(), MAX_FILTER).max(self.state.inv_sample_rate);
            let sync = p
                .trigger
                .as_ref()
                .is_some_and(|t| lane.trigger.process(t.get_value(lane_index)));
            let glide = |glide: &mut SwapGlide, value: f32, mark: GlideMark| {
                glide.follow(value, mark, sample_rate)
            };
            let knobs = [
                p.rate.value_or(lane_index, MiNoiseParams::RATE_DEFAULT),
                glide(
                    &mut lane.mode_glide,
                    p.mode.value_or(lane_index, MiNoiseParams::MODE_DEFAULT),
                    p.mode.glide_mark(lane_index),
                ),
                glide(
                    &mut lane.resonance_glide,
                    p.resonance
                        .value_or(lane_index, MiNoiseParams::RESONANCE_DEFAULT),
                    p.resonance.glide_mark(lane_index),
                ),
            ];
            lane.control(f0, knobs, patched, &self.state);
            let (out, aux) = lane.step(sync, &self.state);
            // Plaits clips after its slow limiter.
            self.outputs
                .sample
                .set(lane_index, (OUTPUT_GAIN * out).clamp(-5.0, 5.0));
            self.outputs
                .bands
                .set(lane_index, (OUTPUT_GAIN * aux).clamp(-5.0, 5.0));
        }
    }
}

impl crate::types::PatchUpdateHandler for MiNoise {
    /// Seeds the random generators of lanes that have none yet. Runs here,
    /// not in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same noise.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (lane_index, lane) in self.lane_state.iter_mut().enumerate() {
            if !lane.seeded {
                let unit = (hash_bipolar(base, lane_index as u64) + 1.0) * 0.5;
                lane.random = Random::new((unit * f64::from(u32::MAX)) as u32);
                lane.seeded = true;
            }
        }
    }
}

message_handlers!(impl MiNoise {});

#[cfg(test)]
mod tests;
