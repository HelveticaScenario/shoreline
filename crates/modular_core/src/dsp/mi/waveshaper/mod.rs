use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            interpolate::hermite,
            oscillator::SlopeOscillator,
            phase::{limit_frequency, max_frequency},
            sine::sine,
            tables::{
                LUT_FOLD, LUT_FOLD_2, LUT_WS_BUMP, LUT_WS_DOUBLE_BUMP, LUT_WS_INVERSE_SIN,
                LUT_WS_INVERSE_TAN, LUT_WS_LINEAR,
            },
        },
        oscillators::{FmMode, apply_fm},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

/// Plaits' output gains for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.7 * 5.0;
const OVERTONES_GAIN: f32 = -0.6 * 5.0;

/// The shaper curves `shape` morphs across. `shape` stops just short of 4,
/// so it always reads a pair.
const SHAPERS: [&[i16; 257]; 5] = [
    &LUT_WS_INVERSE_TAN,
    &LUT_WS_INVERSE_SIN,
    &LUT_WS_LINEAR,
    &LUT_WS_BUMP,
    &LUT_WS_DOUBLE_BUMP,
];

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct WaveshaperParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "shaper curve: 2.5 leaves the wave as it is; toward 0 it rounds into arctangent and sine curves, toward 5 it adds bumps",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    shape: Option<PolySignal>,
    #[input(
        "wavefolder drive: folds the shaped wave back on itself for a brighter, buzzier tone; also brings in `overtones`",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    fold: Option<PolySignal>,
    #[input(
        "the source wave's lean, from a triangle (0) toward a ramp (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    asymmetry: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs the waves backward",
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
struct WaveshaperOutputs {
    #[output("shaped", "the shaped and folded wave", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
    #[output(
        "overtones",
        "a sine that `fold` crossfades into a second wavefolder's tone",
        type = cv,
        range = (-5.0, 5.0),
    )]
    overtones: PolyOutput,
}

const WAVESHAPER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Waveshaping oscillator",
    description: "Mutable Instruments Plaits' waveshaping oscillator: a \
                  triangle bent through a morphing shaper curve and a \
                  wavefolder, for hollow, reedy and buzzy West Coast tones.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Folding bass",
            code: r#"
                const drive = $sine('0.25hz').range(0.5, 4)
                $mi.waveshaper('c2', { fold: drive }).out()
            "#,
        },
        DocExample {
            title: "Bumpy overtones",
            code: r#"
                const shaper = $mi.waveshaper('g2', { shape: 4, fold: 3 })
                shaper.o.overtones.out()
            "#,
        },
    ],
};

#[module(name = "$mi.waveshaper", docs = WAVESHAPER_DOCS, args(freq), has_init)]
pub struct Waveshaper {
    outputs: WaveshaperOutputs,
    state: WaveshaperState,
    channel_state: Box<[Voice]>,
    params: WaveshaperParams,
}

#[derive(Default)]
struct WaveshaperState {
    sample_rate: f32,
    inv_sample_rate: f32,
    max_frequency: f32,
    /// Cycles per sample here to cycles per sample at Plaits' 48 kHz, so the
    /// aliasing guards act at the same pitch at any rate.
    to_48k: f32,
}

impl WaveshaperState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            max_frequency: max_frequency(sample_rate),
            to_48k: sample_rate / 48000.0,
        }
    }
}

/// The controls for one sample.
struct Controls {
    /// Pitch in cycles per sample.
    f0: f32,
    pw: f32,
    /// Position across `SHAPERS`, 0 to just under 4.
    shape: f32,
    wavefolder_gain: f32,
    overtone_gain: f32,
}

#[derive(Clone, Copy)]
struct Voice {
    slope: SlopeOscillator,
    triangle: SlopeOscillator,
    shape: SwapGlide,
    fold: SwapGlide,
    asymmetry: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 3],
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            slope: SlopeOscillator::default(),
            triangle: SlopeOscillator::default(),
            shape: SwapGlide::default(),
            fold: SwapGlide::default(),
            asymmetry: SwapGlide::default(),
            held: [
                WaveshaperParams::SHAPE_DEFAULT,
                WaveshaperParams::FOLD_DEFAULT,
                WaveshaperParams::ASYMMETRY_DEFAULT,
            ],
        }
    }
}

/// The control inputs for a sample: the raw signals and their glide marks.
struct Knobs {
    shape: (f32, GlideMark),
    fold: (f32, GlideMark),
    asymmetry: (f32, GlideMark),
}

/// Plaits' `Tame`: how much of a control survives at pitch `f0` (cycles per
/// sample at 48 kHz) for a tone whose harmonics reach `harmonics` × `order`.
#[inline(always)]
fn tame(f0: f32, harmonics: f32, order: f32) -> f32 {
    let f0 = f0 * harmonics;
    let max_f = 0.5 / order;
    let amount = (1.0 - (f0 - max_f) / (0.5 - max_f)).clamp(0.0, 1.0);
    amount * amount * amount
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Voice {
    #[inline(always)]
    fn control(&mut self, f0: f32, knobs: Knobs, state: &WaveshaperState) -> Controls {
        let sample_rate = state.sample_rate;
        let raw = [
            self.shape.follow(knobs.shape.0, knobs.shape.1, sample_rate),
            self.fold.follow(knobs.fold.0, knobs.fold.1, sample_rate),
            self.asymmetry
                .follow(knobs.asymmetry.0, knobs.asymmetry.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let harmonics = knob(self.held[0]);
        let timbre = knob(self.held[1]);
        let morph = knob(self.held[2]);

        let f0 = limit_frequency(f0, state.max_frequency);
        let pitch = f0.abs() * state.to_48k;
        let slope = 3.0 + (morph - 0.5).abs() * 5.0;
        let shape_amount = (harmonics - 0.5).abs() * 2.0;
        let shape_attenuation = tame(pitch, slope, 16.0);
        let wavefolder_attenuation = tame(
            pitch,
            slope * (3.0 + shape_amount * shape_attenuation * 5.0),
            12.0,
        );
        let overtone = timbre * (2.0 - timbre);
        Controls {
            f0,
            pw: morph * 0.45 + 0.5,
            shape: (0.5 + (harmonics - 0.5) * shape_attenuation) * 3.9999,
            wavefolder_gain: 0.03 + 0.46 * timbre * wavefolder_attenuation,
            overtone_gain: overtone * (2.0 - overtone),
        }
    }

    /// One sample of the folded wave and the overtones, before the output
    /// gains.
    #[inline(always)]
    fn step(&mut self, c: Controls) -> (f32, f32) {
        let slope = self.slope.step(c.f0, c.pw);
        let triangle = self.triangle.step(c.f0, 0.5);

        let shape_integral = c.shape as i32;
        let shape_fractional = c.shape - shape_integral as f32;
        let shape_1 = SHAPERS[shape_integral as usize];
        let shape_2 = SHAPERS[shape_integral as usize + 1];
        let ws_index = 127.0 * slope + 128.0;
        let ws_integral = ws_index as i32;
        let ws_fractional = ws_index - ws_integral as f32;
        let i = (ws_integral & 255) as usize;
        let read = |table: &[i16; 257]| {
            let a = f32::from(table[i]) / 32768.0;
            let b = f32::from(table[i + 1]) / 32768.0;
            a + (b - a) * ws_fractional
        };
        let x = read(shape_1);
        let y = read(shape_2);
        let mix = x + (y - x) * shape_fractional;
        let index = mix * c.wavefolder_gain + 0.5;
        let fold = hermite(&LUT_FOLD[1..], index, 512.0);
        let fold_2 = -hermite(&LUT_FOLD_2[1..], index, 512.0);
        let sine = sine(triangle * 0.25 + 0.5);
        (fold, sine + (fold_2 - sine) * c.overtone_gain)
    }
}

impl Waveshaper {
    fn init(&mut self, sample_rate: f32) {
        self.state = WaveshaperState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        for (ch, voice) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, WaveshaperParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                shape: (
                    p.shape.value_or(ch, WaveshaperParams::SHAPE_DEFAULT),
                    p.shape.glide_mark(ch),
                ),
                fold: (
                    p.fold.value_or(ch, WaveshaperParams::FOLD_DEFAULT),
                    p.fold.glide_mark(ch),
                ),
                asymmetry: (
                    p.asymmetry
                        .value_or(ch, WaveshaperParams::ASYMMETRY_DEFAULT),
                    p.asymmetry.glide_mark(ch),
                ),
            };
            let controls = voice.control(f0, knobs, &self.state);
            let (out, overtones) = voice.step(controls);
            self.outputs.sample.set(ch, OUTPUT_GAIN * out);
            self.outputs.overtones.set(ch, OVERTONES_GAIN * overtones);
        }
    }
}

message_handlers!(impl Waveshaper {});

#[cfg(test)]
mod tests;
