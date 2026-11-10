mod terrain;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{fast_sine::QuadratureOscillator, phase::limit_frequency, sine::sine},
        oscillators::{FmMode, apply_fm},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use terrain::{TERRAINS, terrain};

/// Plaits' output gains for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.7 * 5.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct WaveTerrainParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "the surface scanned, blending through eight: five smooth mathematical landscapes, then three built from wavetables",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    terrain: Option<PolySignal>,
    #[input(
        "size of the circle traced over the surface: a narrow, mellow loop (0) to the whole landscape (5); shrunk at high pitches to keep it from aliasing",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    radius: Option<PolySignal>,
    #[input(
        "where across the surface the circle sits, from one edge (0) through the middle (2.6) to the other (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    offset: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero traces the circle backward",
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
struct WaveTerrainOutputs {
    #[output(
        "output",
        "the height of the surface under the circle",
        type = cv,
        default,
        range = (-5.0, 5.0),
    )]
    sample: PolyOutput,
    #[output(
        "pm",
        "a sine phase-modulated by the circle and the height, rounder and more vocal",
        type = cv,
        range = (-5.0, 5.0),
    )]
    pm: PolyOutput,
}

const WAVE_TERRAIN_DOCS: ModuleDocs = ModuleDocs {
    summary: "Wave terrain oscillator",
    description: "Mutable Instruments Plaits' wave terrain synthesis: a \
                  point circles over a 3D surface once per cycle and the \
                  surface's height is the waveform, so moving the circle or \
                  morphing the surface reshapes the tone.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Drifting over the landscapes",
            code: r#"
                const drift = $sine('0.05hz').range(0, 5)
                $mi.waveTerrain('c2', { terrain: drift }).out()
            "#,
        },
        DocExample {
            title: "Phase-modulated voice",
            code: "$mi.waveTerrain('g2', { radius: 4, offset: 1 }).pm.out()",
        },
    ],
};

#[module(name = "$mi.waveTerrain", docs = WAVE_TERRAIN_DOCS, args(freq), has_init)]
pub struct WaveTerrain {
    outputs: WaveTerrainOutputs,
    state: WaveTerrainState,
    channel_state: Box<[Voice]>,
    params: WaveTerrainParams,
}

#[derive(Default)]
struct WaveTerrainState {
    sample_rate: f32,
    inv_sample_rate: f32,
    to_48k: f32,
}

impl WaveTerrainState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
        }
    }
}

/// The controls for a sample.
#[derive(Clone, Copy, Default)]
struct Controls {
    /// Path frequency, in cycles per substep, and radius.
    frequency: f32,
    amplitude: f32,
    x_offset: f32,
    /// The two terrains blended, and how far toward the second.
    terrain: usize,
    blend: f32,
}

#[derive(Clone, Copy)]
struct Voice {
    path: QuadratureOscillator,
    controls: Controls,
    /// The pitch and knobs, as bits, `controls` were derived from.
    last_inputs: [u32; 4],
    terrain: SwapGlide,
    radius: SwapGlide,
    offset: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 3],
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            path: QuadratureOscillator::default(),
            controls: Controls::default(),
            last_inputs: [f32::NAN.to_bits(); 4],
            terrain: SwapGlide::default(),
            radius: SwapGlide::default(),
            offset: SwapGlide::default(),
            held: [
                WaveTerrainParams::TERRAIN_DEFAULT,
                WaveTerrainParams::RADIUS_DEFAULT,
                WaveTerrainParams::OFFSET_DEFAULT,
            ],
        }
    }
}

/// The control inputs for a sample: the raw signals and their glide marks.
struct Knobs {
    terrain: (f32, GlideMark),
    radius: (f32, GlideMark),
    offset: (f32, GlideMark),
}

fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Voice {
    /// The controls for a sample at `f0` cycles per sample, rederived only
    /// when the pitch or a knob moves or a glide is under way.
    #[inline(always)]
    fn control(&mut self, f0: f32, knobs: Knobs, state: &WaveTerrainState) -> Controls {
        let inputs = [f0, knobs.terrain.0, knobs.radius.0, knobs.offset.0].map(f32::to_bits);
        if inputs == self.last_inputs
            && !self.terrain.will_glide(knobs.terrain.1)
            && !self.radius.will_glide(knobs.radius.1)
            && !self.offset.will_glide(knobs.offset.1)
        {
            return self.controls;
        }
        self.last_inputs = inputs;
        let sample_rate = state.sample_rate;
        let raw = [
            self.terrain
                .follow(knobs.terrain.0, knobs.terrain.1, sample_rate),
            self.radius
                .follow(knobs.radius.0, knobs.radius.1, sample_rate),
            self.offset
                .follow(knobs.offset.0, knobs.offset.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let [harmonics, timbre, morph] = self.held.map(knob);

        let f0 = limit_frequency(f0, 0.5);
        // Plaits' 48 kHz pitch, or this rate's when that is nearer Nyquist:
        // the anti-aliasing below keeps Plaits' tone and still stops short of
        // this rate's Nyquist.
        let speed = f0.abs().max(f0.abs() * state.to_48k);
        let attenuation = (1.0 - 8.0 * speed).max(0.0);
        let radius = 0.1 + 0.9 * timbre * attenuation * (2.0 - attenuation);
        // The circle turns at half the pitch, twice per sample.
        let path_speed = speed * 0.5;
        let amplitude = if path_speed >= 0.25 {
            0.0
        } else {
            radius * (1.0 - path_speed * 4.0)
        };
        let z = (harmonics * 1.05).min(1.0) * (TERRAINS as f32 - 1.0001);
        let terrain = z as usize;
        self.controls = Controls {
            frequency: f0 * 0.5,
            amplitude,
            x_offset: 1.9 * morph - 1.0,
            terrain,
            blend: z - terrain as f32,
        };
        self.controls
    }

    /// One sample: the height and the phase-modulated sine, before the
    /// output gain.
    #[inline(always)]
    fn step(&mut self, c: Controls) -> (f32, f32) {
        let path = self.path.render::<2>(c.frequency, c.amplitude);
        let mut out = 0.0;
        let mut aux = 0.0;
        for (x, y) in path {
            let x = x * (1.0 - c.x_offset.abs()) + c.x_offset;
            let z0 = terrain(x, y, c.terrain);
            let z1 = terrain(x, y, c.terrain + 1);
            let height = z0 + (z1 - z0) * c.blend;
            out += height;
            aux += y + height;
        }
        (0.5 * out, sine(1.0 + 0.5 * 0.5 * aux))
    }
}

impl WaveTerrain {
    fn init(&mut self, sample_rate: f32) {
        self.state = WaveTerrainState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        for (ch, voice) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, WaveTerrainParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                terrain: (
                    p.terrain.value_or(ch, WaveTerrainParams::TERRAIN_DEFAULT),
                    p.terrain.glide_mark(ch),
                ),
                radius: (
                    p.radius.value_or(ch, WaveTerrainParams::RADIUS_DEFAULT),
                    p.radius.glide_mark(ch),
                ),
                offset: (
                    p.offset.value_or(ch, WaveTerrainParams::OFFSET_DEFAULT),
                    p.offset.glide_mark(ch),
                ),
            };
            let controls = voice.control(f0, knobs, &self.state);
            let (out, pm) = voice.step(controls);
            self.outputs.sample.set(ch, OUTPUT_GAIN * out);
            self.outputs.pm.set(ch, OUTPUT_GAIN * pm);
        }
    }
}

message_handlers!(impl WaveTerrain {});

#[cfg(test)]
mod tests;
