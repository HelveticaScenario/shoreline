use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            downsampler::{Downsampler, OVERSAMPLING},
            ratio::{ratio_multiplier, ratio_semitones},
            sine::{phase_increment, sine_pm},
        },
        oscillators::{FmMode, apply_fm},
        utils::changed,
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.6 * 5.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct TwoOpParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "modulator frequency against the carrier, from an octave below (0) to three octaves above (5), lingering on harmonic ratios",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    ratio: Option<PolySignal>,
    #[input(
        "modulation depth: how bright and clangorous the tone gets; eased off at high pitches to keep it from aliasing",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    index: Option<PolySignal>,
    #[input(
        "feedback of the output into the modulator: positive into its phase for a harsher, noisier tone; negative into its pitch for a growl",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    feedback: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs both operators backward",
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
struct TwoOpOutputs {
    #[output("output", "the FM tone", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
    #[output(
        "subOctave",
        "a sine an octave down, phase-modulated by the FM tone",
        type = cv,
        range = (-5.0, 5.0),
    )]
    sub_octave: PolyOutput,
}

const TWO_OP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Two-operator FM oscillator",
    description: "Mutable Instruments Plaits' two-operator FM: one sine \
                  phase-modulating another, 4× oversampled, for bells, \
                  electric pianos, metallic basses and clangorous leads.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Evolving FM bass",
            code: r#"
                const bright = $sine('0.2hz').range(0, 4)
                $mi.twoOp('c2', { index: bright }).out()
            "#,
        },
        DocExample {
            title: "Sub-octave growl",
            code: "$mi.twoOp('c1', { index: 3, feedback: -3 }).subOctave.out()",
        },
    ],
};

#[module(name = "$mi.twoOp", docs = TWO_OP_DOCS, args(freq), has_init)]
pub struct TwoOp {
    outputs: TwoOpOutputs,
    state: TwoOpState,
    channel_state: Box<[Voice]>,
    params: TwoOpParams,
}

#[derive(Default)]
struct TwoOpState {
    sample_rate: f32,
    inv_sample_rate: f32,
    /// Highest modulator frequency, in cycles per substep: Plaits' 0.5 at
    /// 48 kHz, held in Hz.
    max_modulator: f32,
    /// The one-pole that smooths the output for feedback, per substep.
    feedback_smoothing: f32,
}

impl TwoOpState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            max_modulator: 0.5 * 48000.0 / sample_rate,
            feedback_smoothing: (1.0 - 0.95f64.powf(48000.0 / f64::from(sample_rate))) as f32,
        }
    }
}

/// The controls for one sample.
#[derive(Clone, Copy, Default)]
struct Controls {
    /// Carrier frequency, in cycles per substep.
    carrier: f32,
    /// Modulator frequency, in cycles per substep.
    modulator: f32,
    amount: f32,
    feedback: f32,
}

#[derive(Clone, Copy)]
struct Voice {
    carrier_phase: u32,
    modulator_phase: u32,
    sub_phase: u32,
    /// The carrier smoothed by a one-pole, fed back into the modulator.
    previous_sample: f32,
    carrier_fir: Downsampler,
    sub_fir: Downsampler,
    ratio: SwapGlide,
    index: SwapGlide,
    feedback: SwapGlide,
    /// Modulator ratio for the cached `ratio` knob, in semitones and as a
    /// frequency multiplier; `NaN` = never computed.
    last_ratio: f32,
    semitones: f32,
    multiplier: f32,
    /// High-pitch index reduction for the cached pitch and ratio; `NaN` =
    /// never computed.
    last_pitch: f32,
    last_semitones: f32,
    hf_taming: f32,
    /// The last finite `index` and `feedback`, which a NaN or infinite knob
    /// holds.
    held_index: f32,
    held_feedback: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            carrier_phase: 0,
            modulator_phase: 0,
            sub_phase: 0,
            previous_sample: 0.0,
            carrier_fir: Downsampler::default(),
            sub_fir: Downsampler::default(),
            ratio: SwapGlide::default(),
            index: SwapGlide::default(),
            feedback: SwapGlide::default(),
            last_ratio: f32::NAN,
            semitones: ratio_semitones(TwoOpParams::RATIO_DEFAULT),
            multiplier: ratio_multiplier(ratio_semitones(TwoOpParams::RATIO_DEFAULT)),
            last_pitch: f32::NAN,
            last_semitones: f32::NAN,
            hf_taming: 1.0,
            held_index: TwoOpParams::INDEX_DEFAULT,
            held_feedback: TwoOpParams::FEEDBACK_DEFAULT,
        }
    }
}

/// The control inputs for a sample: the raw signals and their glide marks.
struct Knobs {
    ratio: (f32, GlideMark),
    index: (f32, GlideMark),
    feedback: (f32, GlideMark),
}

impl Voice {
    /// The controls for a sample at `f0` cycles per sample, recomputing the
    /// costly ones only when their inputs move.
    #[inline(always)]
    fn control(&mut self, f0: f32, knobs: Knobs, state: &TwoOpState) -> Controls {
        let sample_rate = state.sample_rate;
        let ratio = self.ratio.follow(knobs.ratio.0, knobs.ratio.1, sample_rate);
        let index = self.index.follow(knobs.index.0, knobs.index.1, sample_rate);
        let feedback = self
            .feedback
            .follow(knobs.feedback.0, knobs.feedback.1, sample_rate);

        // A NaN or infinite knob holds its control where it is.
        if index.is_finite() {
            self.held_index = index;
        }
        if feedback.is_finite() {
            self.held_feedback = feedback;
        }
        if changed(ratio, self.last_ratio) && ratio.is_finite() {
            self.semitones = ratio_semitones(ratio);
            self.multiplier = ratio_multiplier(self.semitones);
            self.last_ratio = ratio;
        }
        let pitch = f0.abs();
        if changed(pitch, self.last_pitch) || changed(self.semitones, self.last_semitones) {
            // The modulator's MIDI note, in Plaits' two-octave-down substep
            // terms.
            // f64 then rounded, so every platform's libm gives the same value.
            let note = 69.0 + 12.0 * f64::from(pitch * sample_rate / 440.0).log2() as f32;
            let taming = (1.0 - (note - 24.0 + self.semitones - 72.0) * 0.025).clamp(0.0, 1.0);
            self.hf_taming = taming * taming;
            self.last_pitch = pitch;
            self.last_semitones = self.semitones;
        }

        let carrier = f0 * 0.25;
        let timbre = (self.held_index * 0.2).clamp(0.0, 1.0);
        Controls {
            carrier,
            modulator: (carrier * self.multiplier).clamp(-state.max_modulator, state.max_modulator),
            amount: 2.0 * timbre * timbre * self.hf_taming,
            feedback: (self.held_feedback * 0.2).clamp(-1.0, 1.0),
        }
    }

    /// One output sample of the carrier and the sub-octave, before the
    /// output gain.
    #[inline(always)]
    fn step(&mut self, c: Controls, state: &TwoOpState) -> (f32, f32) {
        let phase_feedback = if c.feedback < 0.0 {
            0.5 * c.feedback * c.feedback
        } else {
            0.0
        };
        let modulator_fb = if c.feedback > 0.0 {
            0.25 * c.feedback * c.feedback
        } else {
            0.0
        };
        let carrier_increment = phase_increment(c.carrier);
        let sub_increment = ((carrier_increment as i32) >> 1) as u32;
        for j in 0..OVERSAMPLING {
            self.modulator_phase = self.modulator_phase.wrapping_add(phase_increment(
                c.modulator * (1.0 + self.previous_sample * phase_feedback),
            ));
            self.carrier_phase = self.carrier_phase.wrapping_add(carrier_increment);
            self.sub_phase = self.sub_phase.wrapping_add(sub_increment);
            let modulator = sine_pm(self.modulator_phase, modulator_fb * self.previous_sample);
            let carrier = sine_pm(self.carrier_phase, c.amount * modulator);
            let sub = sine_pm(self.sub_phase, c.amount * carrier * 0.25);
            self.previous_sample += state.feedback_smoothing * (carrier - self.previous_sample);
            self.carrier_fir.accumulate(j, carrier);
            self.sub_fir.accumulate(j, sub);
        }
        (self.carrier_fir.read(), self.sub_fir.read())
    }
}

impl TwoOp {
    fn init(&mut self, sample_rate: f32) {
        self.state = TwoOpState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        for (ch, voice) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, TwoOpParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                ratio: (
                    p.ratio.value_or(ch, TwoOpParams::RATIO_DEFAULT),
                    p.ratio.glide_mark(ch),
                ),
                index: (
                    p.index.value_or(ch, TwoOpParams::INDEX_DEFAULT),
                    p.index.glide_mark(ch),
                ),
                feedback: (
                    p.feedback.value_or(ch, TwoOpParams::FEEDBACK_DEFAULT),
                    p.feedback.glide_mark(ch),
                ),
            };
            let controls = voice.control(f0, knobs, &self.state);
            let (out, sub) = voice.step(controls, &self.state);
            self.outputs.sample.set(ch, OUTPUT_GAIN * out);
            self.outputs.sub_octave.set(ch, OUTPUT_GAIN * sub);
        }
    }
}

message_handlers!(impl TwoOp {});

#[cfg(test)]
mod tests;
