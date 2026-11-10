mod oscillator;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            phase::{limit_frequency, max_frequency},
            ratio::{ratio_multiplier, ratio_semitones},
            sine::sine,
        },
        oscillators::{FmMode, apply_fm},
        utils::changed,
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use oscillator::PdOscillator;

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.7 * 5.0;

fn default_true() -> bool {
    true
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct PhaseDistortionParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "the distorting oscillator's frequency against the pitch, from an octave below (0) to three octaves above (5), lingering on harmonic ratios",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    ratio: Option<PolySignal>,
    #[input(
        "how far the triangle bends the phase: from none (0) to bright, resonant and harsh",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    amount: Option<PolySignal>,
    #[input(
        "leans the distorting triangle and, when synced, the phase itself, from symmetric (0) to lopsided (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    asymmetry: Option<PolySignal>,
    #[input(
        "restart the distorting oscillator every cycle, for a stable, resonant-filter-like sweep; off, it runs free, beating and inharmonic between the harmonic ratios"
    )]
    #[serde(default = "default_true")]
    #[deserr(default = default_true())]
    sync: bool,
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
struct PhaseDistortionOutputs {
    #[output("output", "the phase-distorted sine", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

const PHASE_DISTORTION_DOCS: ModuleDocs = ModuleDocs {
    summary: "Phase distortion oscillator",
    description: "Mutable Instruments Plaits' phase distortion oscillator, in \
                  the spirit of Casio's CZ synths: a sine read through a phase \
                  bent by a second oscillator, for resonant sweeps, buzzy \
                  leads and hollow basses.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Resonant sweep",
            code: r#"
                const sweep = $sine('0.2hz').range(0.5, 4.5)
                $mi.phaseDistortion('c2', { amount: sweep }).out()
            "#,
        },
        DocExample {
            title: "Free-running growl",
            code: "$mi.phaseDistortion('a1', { ratio: 3.2, sync: false }).out()",
        },
    ],
};

#[module(name = "$mi.phaseDistortion", docs = PHASE_DISTORTION_DOCS, args(freq), has_init)]
pub struct PhaseDistortion {
    outputs: PhaseDistortionOutputs,
    state: PhaseDistortionState,
    channel_state: Box<[Voice]>,
    params: PhaseDistortionParams,
}

#[derive(Default)]
struct PhaseDistortionState {
    sample_rate: f32,
    inv_sample_rate: f32,
    /// Highest frequency, in cycles per substep: Plaits' quarter cycle at
    /// its 96 kHz oversampled rate, held in Hz.
    max_frequency: f32,
    /// Cycles per substep here to cycles per substep at Plaits' 96 kHz.
    to_48k: f32,
}

impl PhaseDistortionState {
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
    /// Carrier and modulator frequencies, in cycles per substep.
    carrier: f32,
    modulator: f32,
    pw: f32,
    amount: f32,
}

#[derive(Clone, Copy)]
struct Voice {
    oscillator: PdOscillator,
    ratio: SwapGlide,
    amount: SwapGlide,
    asymmetry: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 3],
    /// Modulator multiplier for the cached ratio knob; `NaN` = never computed.
    last_ratio: f32,
    multiplier: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            oscillator: PdOscillator::default(),
            ratio: SwapGlide::default(),
            amount: SwapGlide::default(),
            asymmetry: SwapGlide::default(),
            held: [
                PhaseDistortionParams::RATIO_DEFAULT,
                PhaseDistortionParams::AMOUNT_DEFAULT,
                PhaseDistortionParams::ASYMMETRY_DEFAULT,
            ],
            last_ratio: f32::NAN,
            multiplier: 1.0,
        }
    }
}

/// The control inputs for a sample: the raw signals and their glide marks.
struct Knobs {
    ratio: (f32, GlideMark),
    amount: (f32, GlideMark),
    asymmetry: (f32, GlideMark),
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Voice {
    #[inline(always)]
    fn control(&mut self, f0: f32, knobs: Knobs, state: &PhaseDistortionState) -> Controls {
        let sample_rate = state.sample_rate;
        let raw = [
            self.ratio.follow(knobs.ratio.0, knobs.ratio.1, sample_rate),
            self.amount
                .follow(knobs.amount.0, knobs.amount.1, sample_rate),
            self.asymmetry
                .follow(knobs.asymmetry.0, knobs.asymmetry.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        if changed(self.held[0], self.last_ratio) {
            self.multiplier = ratio_multiplier(ratio_semitones(self.held[0]));
            self.last_ratio = self.held[0];
        }
        let timbre = knob(self.held[1]);
        let morph = knob(self.held[2]);

        let carrier = 0.5 * limit_frequency(f0, 2.0 * state.max_frequency);
        let modulator =
            (carrier * self.multiplier).clamp(-state.max_frequency, state.max_frequency);
        Controls {
            carrier,
            modulator,
            pw: 0.5 + morph * 0.49,
            amount: 8.0 * timbre * timbre * (1.0 - modulator.abs() * state.to_48k * 3.8),
        }
    }

    /// One output sample, before the output gain.
    #[inline(always)]
    fn step(&mut self, c: Controls, sync: bool, state: &PhaseDistortionState) -> f32 {
        let phases = self.oscillator.render(
            sync,
            c.carrier,
            c.modulator,
            c.pw,
            c.amount,
            state.max_frequency,
        );
        let mut out = 0.0;
        for phase in phases {
            out += 0.5 * sine(phase + 0.25);
        }
        out
    }
}

impl PhaseDistortion {
    fn init(&mut self, sample_rate: f32) {
        self.state = PhaseDistortionState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        for (ch, voice) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, PhaseDistortionParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                ratio: (
                    p.ratio.value_or(ch, PhaseDistortionParams::RATIO_DEFAULT),
                    p.ratio.glide_mark(ch),
                ),
                amount: (
                    p.amount.value_or(ch, PhaseDistortionParams::AMOUNT_DEFAULT),
                    p.amount.glide_mark(ch),
                ),
                asymmetry: (
                    p.asymmetry
                        .value_or(ch, PhaseDistortionParams::ASYMMETRY_DEFAULT),
                    p.asymmetry.glide_mark(ch),
                ),
            };
            let controls = voice.control(f0, knobs, &self.state);
            let out = voice.step(controls, p.sync, &self.state);
            self.outputs.sample.set(ch, OUTPUT_GAIN * out);
        }
    }
}

message_handlers!(impl PhaseDistortion {});

#[cfg(test)]
mod tests;
