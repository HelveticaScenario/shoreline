use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            control::Ramp,
            dsp::soft_clip,
            filter::{Svf, tan_fast},
            phase::{limit_frequency, max_frequency},
            variable_shape::VariableShapeOscillator,
        },
        oscillators::{FmMode, apply_fm},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -5.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct VcfParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "filter cutoff, following the pitch: from two octaves below it (0) to eight above (5), up to 12 kHz",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    cutoff: Option<PolySignal>,
    #[input(
        "filter resonance, rising away from 0: below 0 a steep 4-pole filter, above about 1.5 a gentler 2-pole one that is driven harder",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    resonance: Option<PolySignal>,
    #[input(
        "the waveform: a saw with a sub-octave square (0), a saw, a square (2.5), a narrow pulse, then a square with the sub (5)",
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
struct VcfOutputs {
    #[output("lowpass", "the filtered voice", type = cv, default, range = (-5.0, 5.0))]
    lowpass: PolyOutput,
    #[output(
        "highpass",
        "the same filter's high-pass output, thin and buzzy",
        type = cv,
        range = (-5.0, 5.0),
    )]
    highpass: PolyOutput,
}

const VCF_DOCS: ModuleDocs = ModuleDocs {
    summary: "Oscillator into a resonant filter",
    description: "Mutable Instruments Plaits' virtual analog VCF voice: a \
                  saw, square or narrow pulse, with a sub-octave square to \
                  mix in, through a resonant filter that follows the pitch. \
                  `resonance` is bipolar: its sign picks a steep or a gentle \
                  filter, and resonance rises either side of 0.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Resonant sweep",
            code: r#"
                const sweep = $sine('0.25hz').range(1, 4)
                bass: $mi.vcf('c2', { cutoff: sweep, resonance: -3 })
            "#,
        },
        DocExample {
            title: "Sub bass",
            code: "bass: $mi.vcf('a1', { shape: 0, cutoff: 1.5 })",
        },
        DocExample {
            title: "High-pass buzz",
            code: "buzz: $mi.vcf('c3', { cutoff: 3, resonance: 3 })._highpass",
        },
    ],
};

#[module(name = "$mi.vcf", docs = VCF_DOCS, args(freq), has_init)]
pub struct Vcf {
    outputs: VcfOutputs,
    state: VcfState,
    lane_state: Box<[Voice]>,
    params: VcfParams,
}

#[derive(Default)]
struct VcfState {
    sample_rate: f32,
    inv_sample_rate: f32,
    /// Highest oscillator and cutoff frequency, in cycles per sample: Plaits'
    /// 12 kHz, kept below the quarter rate where its `tan` approximation and
    /// pulse widths hold.
    max_frequency: f32,
}

impl VcfState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            max_frequency: max_frequency(sample_rate).min(0.25),
        }
    }
}

/// The controls for one sample.
struct Controls {
    /// Oscillator frequency, in cycles per sample.
    f0: f32,
    pw: f32,
    shape: f32,
    sub_gain: f32,
    cutoff: f32,
    stage2_gain: f32,
    q: f32,
    gain: f32,
}

#[derive(Clone, Copy, SeedSide)]
struct Voice {
    oscillator: VariableShapeOscillator,
    sub: VariableShapeOscillator,
    svf: [Svf; 2],
    cutoff: SwapGlide,
    resonance: SwapGlide,
    shape: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 3],
    /// Cutoff over pitch for the cached cutoff knob; `NaN` = never computed.
    last_cutoff: f32,
    cutoff_ratio: f32,
    /// Plaits' one-sample `ParameterInterpolator`s, kept so the voice matches
    /// Plaits bit for bit.
    ramps: [Ramp; 5],
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            oscillator: VariableShapeOscillator::default(),
            sub: VariableShapeOscillator::default(),
            svf: [Svf::default(); 2],
            cutoff: SwapGlide::default(),
            resonance: SwapGlide::default(),
            shape: SwapGlide::default(),
            held: [
                VcfParams::CUTOFF_DEFAULT,
                VcfParams::RESONANCE_DEFAULT,
                VcfParams::SHAPE_DEFAULT,
            ],
            last_cutoff: f32::NAN,
            cutoff_ratio: 1.0,
            ramps: [Ramp::default(); 5],
        }
    }
}

/// The control inputs for a sample: the raw signals and their glide marks.
struct Knobs {
    cutoff: (f32, GlideMark),
    resonance: (f32, GlideMark),
    shape: (f32, GlideMark),
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Voice {
    #[inline(always)]
    fn control(&mut self, f0: f32, knobs: Knobs, state: &VcfState) -> Controls {
        let sample_rate = state.sample_rate;
        let raw = [
            self.cutoff
                .follow(knobs.cutoff.0, knobs.cutoff.1, sample_rate),
            self.resonance
                .follow(knobs.resonance.0, knobs.resonance.1, sample_rate),
            self.shape.follow(knobs.shape.0, knobs.shape.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let timbre = knob(self.held[0]);
        let harmonics = (self.held[1] * 0.1 + 0.5).clamp(0.0, 1.0);
        let morph = knob(self.held[2]);
        if self.held[0].to_bits() != self.last_cutoff.to_bits() {
            self.cutoff_ratio = (f64::from((timbre - 0.2) * 120.0) / 12.0).exp2() as f32;
            self.last_cutoff = self.held[0];
        }

        let f0 = limit_frequency(f0, state.max_frequency);
        let mut pw = (morph - 0.5) * 2.0 + 0.5;
        if morph > 0.75 {
            pw = 2.5 - morph * 2.0;
        }
        let resonance = 2.667 * ((harmonics - 0.5).abs() - 0.125).max(0.0);
        let resonance_sqr = resonance * resonance;
        Controls {
            f0,
            pw: pw.clamp(0.5, 0.98),
            shape: ((morph - 0.25) * 2.0 + 0.5).clamp(0.5, 1.0),
            sub_gain: ((morph - 0.5).abs() - 0.3).max(0.0) * 5.0,
            cutoff: f0.abs() * self.cutoff_ratio,
            stage2_gain: (1.0 - (harmonics - 0.4) * 4.0).clamp(0.0, 1.0),
            q: resonance_sqr * resonance_sqr * 48.0,
            gain: ((harmonics - 0.7) + 0.85).clamp(0.7 - resonance_sqr * 0.3, 1.0),
        }
    }

    /// One sample's low-pass and high-pass outputs, before the output gain.
    #[inline(always)]
    fn step(&mut self, c: Controls, state: &VcfState) -> (f32, f32) {
        let max = state.max_frequency;
        let osc = self.oscillator.step(c.f0, c.pw, c.shape, max);
        let sub = self.sub.step(c.f0 * 0.501, 0.5, 1.0, max);

        let [sub_gain, cutoff, stage2_gain, q, gain] = &mut self.ramps;
        for (ramp, target) in [
            (&mut *sub_gain, c.sub_gain),
            (&mut *cutoff, c.cutoff),
            (&mut *stage2_gain, c.stage2_gain),
            (&mut *q, c.q),
            (&mut *gain, c.gain),
        ] {
            ramp.set(target, Some(1.0));
        }
        let cutoff = cutoff.next_value().min(max);
        let q = q.next_value();
        let stage2_gain = stage2_gain.next_value();
        let g = tan_fast(cutoff);
        self.svf[0].set_g_q(g, 0.5 + q);
        self.svf[1].set_g_q(g, 0.5 + 0.025 * q);

        let gain = gain.next_value();
        let input = soft_clip((osc + sub * sub_gain.next_value()) * gain);
        let (lp, hp) = self.svf[0].low_and_high_pass(input);
        let mut lp = soft_clip(lp * gain);
        lp += stage2_gain * (soft_clip(self.svf[1].low_pass(lp)) - lp);
        (lp, soft_clip(hp * gain))
    }
}

impl Vcf {
    fn init(&mut self, sample_rate: f32) {
        self.state = VcfState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        for (lane, voice) in self.lane_state[..lanes].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(lane),
                p.fm.value_or(lane, VcfParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                cutoff: (
                    p.cutoff.value_or(lane, VcfParams::CUTOFF_DEFAULT),
                    p.cutoff.glide_mark(lane),
                ),
                resonance: (
                    p.resonance.value_or(lane, VcfParams::RESONANCE_DEFAULT),
                    p.resonance.glide_mark(lane),
                ),
                shape: (
                    p.shape.value_or(lane, VcfParams::SHAPE_DEFAULT),
                    p.shape.glide_mark(lane),
                ),
            };
            let controls = voice.control(f0, knobs, &self.state);
            let (lp, hp) = voice.step(controls, &self.state);
            self.outputs.lowpass.set(lane, OUTPUT_GAIN * lp);
            self.outputs.highpass.set(lane, OUTPUT_GAIN * hp);
        }
    }
}

message_handlers!(impl Vcf {});

#[cfg(test)]
mod tests;
