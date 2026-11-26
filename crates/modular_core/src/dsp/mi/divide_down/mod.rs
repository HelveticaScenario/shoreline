pub mod oscillator;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;
use crate::{
    dsp::{
        mi::core::phase::wrap,
        oscillators::{FmMode, apply_fm},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use oscillator::{DivideDownVoice, fade};

/// Plaits' output gain for its string machine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.8 * 5.0;

/// Plaits' string machine registrations (`string_machine_engine.cc`), from
/// saw to square: the amplitudes of three saw partials and three square
/// partials, interleaved.
pub const REGISTRATIONS: [[f32; 6]; 11] = [
    [1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.5, 0.0, 0.5, 0.0, 0.0, 0.0],
    [0.4, 0.0, 0.2, 0.0, 0.4, 0.0],
    [0.3, 0.0, 0.0, 0.3, 0.0, 0.4],
    [0.3, 0.0, 0.0, 0.0, 0.0, 0.7],
    [0.2, 0.0, 0.0, 0.2, 0.0, 0.6],
    [0.0, 0.2, 0.1, 0.0, 0.2, 0.5],
    [0.0, 0.3, 0.0, 0.3, 0.0, 0.4],
    [0.0, 0.4, 0.0, 0.3, 0.0, 0.3],
    [0.0, 0.5, 0.0, 0.5, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
];

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct DivideDownParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "the mix of saw and square footages, from a bright saw (0) through organ-like mixtures to a hollow square (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    registration: Option<PolySignal>,
    #[input(
        "shifts the whole waveform in time, in cycles of its lowest saw; wraps around",
        type = control,
        default = 0.0,
        range = (0.0, 1.0),
    )]
    #[deserr(default)]
    phase_offset: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs the saws backward",
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
struct DivideDownOutputs {
    #[output("output", "the divided-down tone", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

const DIVIDE_DOWN_DOCS: ModuleDocs = ModuleDocs {
    summary: "Divide-down string-machine oscillator",
    description: "Mutable Instruments Plaits' divide-down oscillator: four \
                  saws an octave apart from one phase, mixed into saw, \
                  square and organ registrations like a string machine's \
                  footages. `$mi.stringMachine` plays chords on it through \
                  a low-pass and `$mi.chorus`.",
    categories: &[ReferenceCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Registration sweep",
            code: r#"
                const sweep = $sine('0.1hz').range(0, 5)
                keys: $mi.divideDown('c3', { registration: sweep })
            "#,
        },
        DocExample {
            title: "String chord",
            code: r#"
                const notes = 'c3:m7'
                chord: $mi.divideDown(notes, { registration: 1 })
            "#,
        },
        DocExample {
            title: "Unison strings",
            code: r#"
                strings: $super('a2', (freq, { phaseOffset }) =>
                  $mi.divideDown(freq, { registration: 1.5, phaseOffset }),
                )
            "#,
        },
    ],
};

#[module(name = "$mi.divideDown", docs = DIVIDE_DOWN_DOCS, args(freq), has_init)]
pub struct DivideDown {
    outputs: DivideDownOutputs,
    state: DivideDownState,
    lane_state: Box<[Voice]>,
    params: DivideDownParams,
}

#[derive(Default)]
struct DivideDownState {
    sample_rate: f32,
    inv_sample_rate: f32,
    to_48k: f32,
    /// Plaits' registration smoothing (0.1 per 12-sample block at 48 kHz) as
    /// a per-sample one-pole coefficient.
    smoothing: f32,
}

impl DivideDownState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            smoothing: (1.0 - 0.9f64.powf(1.0 / 12.0 * 48000.0 / f64::from(sample_rate))) as f32,
        }
    }
}

#[derive(Clone, Copy, SeedSide)]
struct Voice {
    voice: DivideDownVoice,
    registration: SwapGlide,
    offset: SwapGlide,
    /// The last finite registration and phase offset, which a NaN or infinite
    /// value holds.
    held: [f32; 2],
    /// The registration smoothed as Plaits smooths it.
    smoothed: f32,
    /// Whether the voice has played; a fresh one starts at its phase offset
    /// and its target registration, and one carried across a patch update
    /// keeps running from where it is.
    started: bool,
    /// The partial amplitudes of the smoothed registration.
    amplitudes: [f32; 7],
    /// The smoothed registration `amplitudes` were computed for, as bits.
    amplitudes_key: Option<u32>,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            voice: DivideDownVoice::default(),
            registration: SwapGlide::default(),
            offset: SwapGlide::default(),
            held: [
                DivideDownParams::REGISTRATION_DEFAULT,
                DivideDownParams::PHASE_OFFSET_DEFAULT,
            ],
            smoothed: 0.0,
            started: false,
            amplitudes: [0.0; 7],
            amplitudes_key: None,
        }
    }
}

#[derive(Clone, Copy)]
struct Knobs {
    registration: (f32, GlideMark),
    offset: (f32, GlideMark),
}

fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Voice {
    /// One sample at `f0` cycles per sample, before the output gain.
    #[inline(always)]
    fn step(&mut self, f0: f32, knobs: Knobs, state: &DivideDownState) -> f32 {
        let raw = [
            self.registration.follow(
                knobs.registration.0,
                knobs.registration.1,
                state.sample_rate,
            ),
            self.offset
                .follow(knobs.offset.0, knobs.offset.1, state.sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let target = knob(self.held[0]);
        let offset = wrap(self.held[1]);
        if self.started {
            self.smoothed += state.smoothing * (target - self.smoothed);
            // Settled: lands on the target rather than creeping toward it
            // through denormals.
            if (target - self.smoothed).abs() < 1e-6 {
                self.smoothed = target;
            }
        } else {
            self.smoothed = target;
            self.voice.seed(offset);
            self.started = true;
        }

        let key = self.smoothed.to_bits();
        if self.amplitudes_key != Some(key) {
            self.amplitudes = oscillator::registration(&REGISTRATIONS, self.smoothed);
            self.amplitudes_key = Some(key);
        }
        let gain = fade(f0, state.to_48k);
        self.voice
            .step(f0, state.to_48k, &self.amplitudes, key, gain, offset)
    }
}

impl DivideDown {
    fn init(&mut self, sample_rate: f32) {
        self.state = DivideDownState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        for (lane, voice) in self.lane_state[..lanes].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(lane),
                p.fm.value_or(lane, DivideDownParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                registration: (
                    p.registration
                        .value_or(lane, DivideDownParams::REGISTRATION_DEFAULT),
                    p.registration.glide_mark(lane),
                ),
                offset: (
                    p.phase_offset
                        .value_or(lane, DivideDownParams::PHASE_OFFSET_DEFAULT),
                    p.phase_offset.glide_mark(lane),
                ),
            };
            let out = voice.step(f0, knobs, &self.state);
            self.outputs.sample.set(lane, OUTPUT_GAIN * out);
        }
    }
}

message_handlers!(impl DivideDown {});

#[cfg(test)]
mod tests;
