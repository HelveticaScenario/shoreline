use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;
use crate::{
    dsp::{
        mi::core::{
            blend::Blend,
            control::Ramp,
            dsp::soft_clip,
            filter::{Svf, tan_fast},
            phase::{limit_frequency, max_frequency},
            variable_shape::VariableShapeOscillator,
        },
        oscillators::{FmMode, apply_fm},
        utils::voct_to_hz,
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
        "filter resonance, from none (0) to a sharp ringing peak (5)",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    resonance: Option<PolySignal>,
    #[input(
        "filter slope, from a steep 4-pole filter (0) to a gentler 2-pole one driven harder (5)",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    slope: Option<PolySignal>,
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
    #[input(
        "crossfades `_blend` from `lowpass` into `highpass`",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    mix: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct VcfOutputs {
    #[output(
        "blend",
        "`lowpass` crossfaded into `highpass` by `mix`",
        type = cv,
        default,
        range = (-5.0, 5.0),
    )]
    blend: PolyOutput,
    #[output("lowpass", "the filtered voice", type = cv, range = (-5.0, 5.0))]
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
                  mix in, through a resonant filter that follows the note.",
    categories: &[ReferenceCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Resonant sweep",
            code: r#"
                const sweep = $sine('0.25hz').range(1, 4)
                bass: $mi.vcf('c2', { cutoff: sweep, resonance: 3 })
            "#,
        },
        DocExample {
            title: "Sub bass",
            code: "bass: $mi.vcf('a1', { shape: 0, cutoff: 1.5 })",
        },
        DocExample {
            title: "High-pass buzz",
            code: "buzz: $mi.vcf('c3', { cutoff: 3, slope: 5 })._highpass",
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
    /// 12 kHz, or a quarter of the rate if lower, where its `tan`
    /// approximation and pulse widths hold.
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
    blend: Blend,
    oscillator: VariableShapeOscillator,
    sub: VariableShapeOscillator,
    svf: [Svf; 2],
    cutoff: SwapGlide,
    resonance: SwapGlide,
    slope: SwapGlide,
    shape: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 4],
    /// Cutoff over pitch for the cached cutoff knob; `NaN` = never computed.
    last_cutoff: f32,
    cutoff_ratio: f32,
    /// Plaits' one-sample `ParameterInterpolator`s, which land a hair off
    /// their target; kept so the voice matches Plaits bit for bit.
    ramps: [Ramp; 5],
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            blend: Blend::default(),
            oscillator: VariableShapeOscillator::default(),
            sub: VariableShapeOscillator::default(),
            svf: [Svf::default(); 2],
            cutoff: SwapGlide::default(),
            resonance: SwapGlide::default(),
            slope: SwapGlide::default(),
            shape: SwapGlide::default(),
            held: [
                VcfParams::CUTOFF_DEFAULT,
                VcfParams::RESONANCE_DEFAULT,
                VcfParams::SLOPE_DEFAULT,
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
    slope: (f32, GlideMark),
    shape: (f32, GlideMark),
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Voice {
    #[inline(always)]
    /// Controls at oscillator frequency `f0` (after `fm`) and note frequency
    /// `note` (before it), both in cycles per sample. The cutoff follows the
    /// note, so `fm` sweeps the oscillator under a steady filter.
    fn control(&mut self, f0: f32, note: f32, knobs: Knobs, state: &VcfState) -> Controls {
        let sample_rate = state.sample_rate;
        let raw = [
            self.cutoff
                .follow(knobs.cutoff.0, knobs.cutoff.1, sample_rate),
            self.resonance
                .follow(knobs.resonance.0, knobs.resonance.1, sample_rate),
            self.slope.follow(knobs.slope.0, knobs.slope.1, sample_rate),
            self.shape.follow(knobs.shape.0, knobs.shape.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let timbre = knob(self.held[0]);
        let resonance = knob(self.held[1]);
        let slope = knob(self.held[2]);
        let morph = knob(self.held[3]);
        if self.held[0].to_bits() != self.last_cutoff.to_bits() {
            self.cutoff_ratio = (f64::from((timbre - 0.2) * 120.0) / 12.0).exp2() as f32;
            self.last_cutoff = self.held[0];
        }

        let f0 = limit_frequency(f0, state.max_frequency);
        let mut pw = (morph - 0.5) * 2.0 + 0.5;
        if morph > 0.75 {
            pw = 2.5 - morph * 2.0;
        }
        // Plaits sets slope, resonance and drive from one knob, `harmonics`,
        // whose ends are the steep and the gentle filter at full resonance.
        // The drive follows the `harmonics` that gives this resonance on each
        // side, crossfaded by slope.
        let steep = 0.375 - resonance / 2.667;
        let gentle = 0.625 + resonance / 2.667;
        let harmonics = steep + (gentle - steep) * slope;
        let resonance_sqr = resonance * resonance;
        Controls {
            f0,
            pw: pw.clamp(0.5, 0.98),
            shape: ((morph - 0.25) * 2.0 + 0.5).clamp(0.5, 1.0),
            sub_gain: ((morph - 0.5).abs() - 0.3).max(0.0) * 5.0,
            cutoff: limit_frequency(note, state.max_frequency).abs() * self.cutoff_ratio,
            stage2_gain: 1.0 - slope,
            q: resonance_sqr * resonance_sqr * 48.0,
            gain: ((harmonics - 0.7) + 0.85).clamp(0.7 - resonance_sqr * 0.3, 1.0),
        }
    }

    /// One sample's low-pass and high-pass outputs, before the output gain.
    #[inline(always)]
    fn step(&mut self, c: Controls, state: &VcfState) -> (f32, f32) {
        let osc = self.oscillator.step(c.f0, c.pw, c.shape);
        let sub = self.sub.step(c.f0 * 0.501, 0.5, 1.0);

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
        let cutoff = cutoff.next_value().min(state.max_frequency);
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

    fn update(&mut self, sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        for (lane, voice) in self.lane_state[..lanes].iter_mut().enumerate() {
            let pitch = p.freq.get_value(lane);
            let f0 = apply_fm(pitch, p.fm.value_or(lane, VcfParams::FM_DEFAULT), p.fm_mode)
                * self.state.inv_sample_rate;
            let note = voct_to_hz(pitch) * self.state.inv_sample_rate;
            let knobs = Knobs {
                cutoff: (
                    p.cutoff.value_or(lane, VcfParams::CUTOFF_DEFAULT),
                    p.cutoff.glide_mark(lane),
                ),
                resonance: (
                    p.resonance.value_or(lane, VcfParams::RESONANCE_DEFAULT),
                    p.resonance.glide_mark(lane),
                ),
                slope: (
                    p.slope.value_or(lane, VcfParams::SLOPE_DEFAULT),
                    p.slope.glide_mark(lane),
                ),
                shape: (
                    p.shape.value_or(lane, VcfParams::SHAPE_DEFAULT),
                    p.shape.glide_mark(lane),
                ),
            };
            let controls = voice.control(f0, note, knobs, &self.state);
            let (lp, hp) = voice.step(controls, &self.state);
            let main = OUTPUT_GAIN * lp;
            let aux = OUTPUT_GAIN * hp;
            self.outputs.lowpass.set(lane, main);
            self.outputs.highpass.set(lane, aux);
            let mix = (
                p.mix.value_or(lane, VcfParams::MIX_DEFAULT),
                p.mix.glide_mark(lane),
            );
            self.outputs
                .blend
                .set(lane, voice.blend.step(main, aux, mix, sample_rate));
        }
    }
}

message_handlers!(impl Vcf {});

#[cfg(test)]
mod tests;
