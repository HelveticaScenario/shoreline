use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            phase::{limit_frequency, max_frequency},
            ratio::ratio_multiplier,
            variable_saw::VariableSawOscillator,
            variable_shape::VariableShapeOscillator,
        },
        oscillators::{FmMode, apply_fm},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.8 * 5.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct VirtualAnalogParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "the second oscillator's interval, lingering on unison, fifths and octaves, out to two octaves down or up",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    detune: Option<PolySignal>,
    #[input(
        "on `dual`, the square from silent through narrow pulses to hollow, then hard-synced; on `sync`, how far the sync sweeps",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    square: Option<PolySignal>,
    #[input(
        "on `dual`, the saw from triangle to bright saw to thin spike; on `sync`, the waveform from triangle to narrow pulse",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    shape: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs the waveform backward",
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
struct VirtualAnalogOutputs {
    #[output(
        "dual",
        "a square and a saw, the saw tuned by `detune`",
        type = cv,
        default,
        range = (-5.0, 5.0),
    )]
    dual: PolyOutput,
    #[output(
        "sync",
        "two hard-synced oscillators, `detune` apart, one subtracted from the other",
        type = cv,
        range = (-5.0, 5.0),
    )]
    sync: PolyOutput,
}

const VIRTUAL_ANALOG_DOCS: ModuleDocs = ModuleDocs {
    summary: "Virtual analog dual oscillator",
    description: "Mutable Instruments Plaits' virtual analog oscillator: a \
                  square and a saw, each with a morphing shape, tuned an \
                  interval apart, and a second output of two hard-synced \
                  oscillators for tearing sync leads.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Fifths",
            code: "lead: $mi.va('c3', { detune: 3.1, shape: 1 })",
        },
        DocExample {
            title: "Sync sweep",
            code: r#"
                const sweep = $sine('0.1hz').range(0.5, 4.5)
                lead: $mi.va('a2', { square: sweep })._sync
            "#,
        },
    ],
};

#[module(name = "$mi.va", docs = VIRTUAL_ANALOG_DOCS, args(freq), has_init)]
pub struct VirtualAnalog {
    outputs: VirtualAnalogOutputs,
    state: VirtualAnalogState,
    lane_state: Box<[Voice]>,
    params: VirtualAnalogParams,
}

#[derive(Default)]
struct VirtualAnalogState {
    sample_rate: f32,
    inv_sample_rate: f32,
    /// Plaits' highest frequency, a quarter cycle per sample at 48 kHz, held in
    /// Hz, in cycles per sample.
    max_frequency: f32,
}

impl VirtualAnalogState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            max_frequency: max_frequency(sample_rate),
        }
    }
}

/// Plaits' detune intervals, in semitones, which `detune` lingers on.
const INTERVALS: [f32; 5] = [0.0, 7.01, 12.01, 19.01, 24.01];

#[inline(always)]
fn squash(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

/// The second oscillator's interval in semitones for a `detune` knob (0–1).
fn detuning(knob: f32) -> f32 {
    let detune = (2.05 * knob - 1.025).clamp(-1.0, 1.0);
    let sign = if detune < 0.0 { -1.0 } else { 1.0 };
    let detune = detune * sign * 3.9999;
    let integral = detune as usize;
    let fractional = detune - integral as f32;
    let a = INTERVALS[integral];
    let b = INTERVALS[integral + 1];
    (a + (b - a) * squash(squash(fractional))) * sign
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

/// The frequency ratios a `detune` knob sets, cached on its exact value.
#[derive(Clone, Copy)]
struct DetuneCache {
    knob: u32,
    ratio: f32,
}

/// The frequency ratios a `square` knob sets, cached on its exact value.
#[derive(Clone, Copy)]
struct SquareCache {
    knob: u32,
    /// How far `sync`'s slaves run above their masters.
    sync_ratio: f32,
    /// How far `dual`'s square runs above its master.
    square_sync_ratio: f32,
}

#[derive(Clone, Copy, SeedSide)]
struct Voice {
    primary: VariableShapeOscillator,
    auxiliary: VariableShapeOscillator,
    square: VariableShapeOscillator,
    saw: VariableSawOscillator,
    detune: SwapGlide,
    square_knob: SwapGlide,
    shape: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 3],
    detune_cache: Option<DetuneCache>,
    square_cache: Option<SquareCache>,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            primary: VariableShapeOscillator::default(),
            auxiliary: VariableShapeOscillator::with_master_phase(0.25),
            square: VariableShapeOscillator::default(),
            saw: VariableSawOscillator::default(),
            detune: SwapGlide::default(),
            square_knob: SwapGlide::default(),
            shape: SwapGlide::default(),
            held: [
                VirtualAnalogParams::DETUNE_DEFAULT,
                VirtualAnalogParams::SQUARE_DEFAULT,
                VirtualAnalogParams::SHAPE_DEFAULT,
            ],
            detune_cache: None,
            square_cache: None,
        }
    }
}

/// The control inputs for a sample: the raw signals and their glide marks.
struct Knobs {
    detune: (f32, GlideMark),
    square: (f32, GlideMark),
    shape: (f32, GlideMark),
}

impl Voice {
    /// One sample of `dual` and `sync`, before the output gain, at `f0`
    /// cycles per sample.
    #[inline(always)]
    fn step(&mut self, f0: f32, knobs: Knobs, state: &VirtualAnalogState) -> (f32, f32) {
        let sample_rate = state.sample_rate;
        let raw = [
            self.detune
                .follow(knobs.detune.0, knobs.detune.1, sample_rate),
            self.square_knob
                .follow(knobs.square.0, knobs.square.1, sample_rate),
            self.shape.follow(knobs.shape.0, knobs.shape.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let harmonics = knob(self.held[0]);
        let timbre = knob(self.held[1]);
        let morph = knob(self.held[2]);
        let detune_ratio = match self.detune_cache {
            Some(cache) if cache.knob == harmonics.to_bits() => cache.ratio,
            _ => {
                let ratio = ratio_multiplier(detuning(harmonics));
                self.detune_cache = Some(DetuneCache {
                    knob: harmonics.to_bits(),
                    ratio,
                });
                ratio
            }
        };
        let square = match self.square_cache {
            Some(cache) if cache.knob == timbre.to_bits() => cache,
            _ => {
                let semitones = if timbre < 0.5 {
                    0.0
                } else {
                    (timbre - 0.5) * (timbre - 0.5) * 4.0 * 48.0
                };
                let cache = SquareCache {
                    knob: timbre.to_bits(),
                    sync_ratio: ratio_multiplier(timbre * timbre * 48.0),
                    square_sync_ratio: ratio_multiplier(semitones),
                };
                self.square_cache = Some(cache);
                cache
            }
        };

        let max = state.max_frequency;
        let primary_f = limit_frequency(f0, max);
        let auxiliary_f = limit_frequency(f0 * detune_ratio, max);

        let shape = (morph * 1.5).clamp(0.0, 1.0);
        let pw = (0.5 + (morph - 0.66) * 1.46).clamp(0.5, 0.995);
        let primary =
            self.primary
                .step_synced(primary_f, primary_f * square.sync_ratio, pw, shape, max);
        let auxiliary = self.auxiliary.step_synced(
            auxiliary_f,
            auxiliary_f * square.sync_ratio,
            pw,
            shape,
            max,
        );
        let sync = (auxiliary - primary) * 0.5;

        let square_pw = (1.3 * timbre - 0.15).clamp(0.005, 0.5);
        let square_gain = (timbre * 8.0).min(1.0);
        let saw_pw = if morph < 0.5 {
            morph + 0.5
        } else {
            1.0 - (morph - 0.5) * 2.0
        };
        let saw_pw = (saw_pw * 1.1).clamp(0.005, 1.0);
        let saw_shape = (10.0 - 21.0 * morph).clamp(0.0, 1.0);
        let saw_gain = (8.0 * (1.0 - morph)).clamp(0.02, 1.0);
        let norm = 1.0 / square_gain.max(saw_gain);
        let square_wave = self.square.step_synced(
            primary_f,
            primary_f * square.square_sync_ratio,
            square_pw,
            1.0,
            max,
        );
        let saw = self.saw.step(auxiliary_f, saw_pw, saw_shape, max);
        let dual = saw * (saw_gain * 0.5 * norm) + square_gain * 0.3 * norm * square_wave;
        (dual, sync)
    }
}

impl VirtualAnalog {
    fn init(&mut self, sample_rate: f32) {
        self.state = VirtualAnalogState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        for (lane, voice) in self.lane_state[..lanes].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(lane),
                p.fm.value_or(lane, VirtualAnalogParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                detune: (
                    p.detune.value_or(lane, VirtualAnalogParams::DETUNE_DEFAULT),
                    p.detune.glide_mark(lane),
                ),
                square: (
                    p.square.value_or(lane, VirtualAnalogParams::SQUARE_DEFAULT),
                    p.square.glide_mark(lane),
                ),
                shape: (
                    p.shape.value_or(lane, VirtualAnalogParams::SHAPE_DEFAULT),
                    p.shape.glide_mark(lane),
                ),
            };
            let (dual, sync) = voice.step(f0, knobs, &self.state);
            self.outputs.dual.set(lane, OUTPUT_GAIN * dual);
            self.outputs.sync.set(lane, OUTPUT_GAIN * sync);
        }
    }
}

message_handlers!(impl VirtualAnalog {});

#[cfg(test)]
mod tests;
