mod wavetable;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, DocSection, DocTable, ModuleDocs};
use crate::{
    dsp::{
        mi::{
            core::{
                chords::{ChordBank, Inversion, NUM_VOICES},
                phase::{limit_frequency, max_frequency},
            },
            divide_down::oscillator::{DivideDownVoice, fade, registration},
        },
        oscillators::{FmMode, apply_fm},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
};

use wavetable::WavetableVoice;

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.8 * 5.0;

/// The chord engine's registrations: the amplitudes of three saw partials
/// and three square partials, interleaved, from a square to a bright mixture.
const REGISTRATIONS: [[f32; 6]; 8] = [
    [0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.5, 0.0, 0.5, 0.0, 0.0, 0.0],
    [0.33, 0.0, 0.33, 0.0, 0.33, 0.0],
    [0.33, 0.0, 0.0, 0.33, 0.0, 0.33],
    [0.5, 0.0, 0.0, 0.0, 0.0, 0.5],
    [0.0, 0.5, 0.0, 0.0, 0.0, 0.5],
    [0.0, 0.1, 0.1, 0.0, 0.2, 0.6],
];

/// The waveform knob (0–1) at which each voice crosses into the wavetable,
/// staggered so the chord changes over a note at a time.
const FADE_POINTS: [f32; NUM_VOICES] = [0.55, 0.47, 0.49, 0.51, 0.53];

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiChordsParams {
    #[input("root pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "the chord, numbered as in the Chords table; a voltage between two holds the current chord until it passes the halfway mark",
        type = control,
        default = 10.0,
        range = (0.0, 10.0),
    )]
    #[deserr(default)]
    chord: Option<PolySignal>,
    #[input(
        "the voicing: each volt lifts the four notes an octave, one after another, from close position (0) up through five rotations",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    inversion: Option<PolySignal>,
    #[input(
        "the tone: organ registrations from a bright mixture (0) to a hollow square (2.3), then wavetable waves, which take over a voice at a time from 2.35 to 2.85",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    waveform: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs the voices backward",
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
struct MiChordsOutputs {
    #[output("chord", "the whole chord", type = cv, default, range = (-5.0, 5.0))]
    chord: PolyOutput,
    #[output(
        "root",
        "the root alone, in each octave the voicing plays it, louder: to double it or give it an effect of its own",
        type = cv,
        range = (-5.0, 5.0),
    )]
    root: PolyOutput,
}

/// The chords `ChordBank` numbers, for the docs of the modules that pick one.
pub(crate) const CHORD_TABLE: DocTable = DocTable {
    columns: &["chord", "Name"],
    rows: &[
        &["0", "octave"],
        &["1", "fifth"],
        &["2", "sus4"],
        &["3", "minor"],
        &["4", "minor 7th"],
        &["5", "minor 9th"],
        &["6", "minor 11th"],
        &["7", "6/9"],
        &["8", "major 9th"],
        &["9", "major 7th"],
        &["10", "major"],
    ],
};

const MI_CHORDS_DOCS: ModuleDocs = ModuleDocs {
    summary: "Organ and wavetable chords",
    description: "Mutable Instruments Plaits' chord engine: four-note chords \
                  played on divide-down organ voices or on wavetable voices.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[DocSection {
        title: "Chords",
        body: "`chord` picks one of eleven chords by number.",
        table: Some(CHORD_TABLE),
        examples: &[],
    }],
    examples: &[
        DocExample {
            title: "Minor 7th organ",
            code: "keys: $mi.chords('c3', { chord: 4 })",
        },
        DocExample {
            title: "Rising voicing",
            code: r#"
                const lift = $sine('0.05hz').range(0, 5)
                pad: $mi.chords('a2', { chord: 5, inversion: lift })
            "#,
        },
        DocExample {
            title: "Wavetable chords and their root",
            code: r#"
                const keys = $mi.chords('e3', { chord: 9, waveform: 4 })
                chords: keys
                root: keys._root
            "#,
        },
    ],
};

#[module(name = "$mi.chords", docs = MI_CHORDS_DOCS, args(freq), has_init)]
pub struct MiChords {
    outputs: MiChordsOutputs,
    state: MiChordsState,
    lane_state: Box<[Lane]>,
    params: MiChordsParams,
}

#[derive(Default)]
struct MiChordsState {
    inv_sample_rate: f32,
    to_48k: f32,
    max_frequency: f32,
    /// Plaits' knob smoothing (0.1 per 12-sample block at 48 kHz) as a
    /// per-sample one-pole coefficient.
    smoothing: f32,
}

impl MiChordsState {
    fn new(sample_rate: f32) -> Self {
        Self {
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            // Half a cycle per sample also keeps one wrap enough for the
            // wavetable's phase at rates below Plaits' 24 kHz.
            max_frequency: max_frequency(sample_rate).min(0.5),
            smoothing: (1.0 - 0.9f64.powf(1.0 / 12.0 * 48000.0 / f64::from(sample_rate))) as f32,
        }
    }
}

#[derive(Clone, Copy, SeedSide)]
struct Lane {
    bank: ChordBank,
    /// The last finite chord, inversion and waveform, which a NaN or infinite
    /// knob holds.
    held: [f32; 3],
    /// The inversion and waveform knobs (0–1) smoothed as Plaits smooths
    /// them.
    smoothed: [f32; 2],
    started: bool,
    inversion: Inversion,
    /// The chord and smoothed inversion `inversion` was computed for.
    inversion_key: Option<(usize, u32)>,
    /// The partial amplitudes of the organ registration.
    registration: [f32; 7],
    /// The registration (0–1) `registration` was computed for, as bits.
    registration_key: Option<u32>,
    divide_down: [DivideDownVoice; NUM_VOICES],
    wavetable: [WavetableVoice; NUM_VOICES],
}

impl Default for Lane {
    fn default() -> Self {
        Self {
            bank: ChordBank::default(),
            held: [
                MiChordsParams::CHORD_DEFAULT,
                MiChordsParams::INVERSION_DEFAULT,
                MiChordsParams::WAVEFORM_DEFAULT,
            ],
            smoothed: [0.0; 2],
            started: false,
            inversion: Inversion::default(),
            inversion_key: None,
            registration: [0.0; 7],
            registration_key: None,
            divide_down: [DivideDownVoice::default(); NUM_VOICES],
            wavetable: [WavetableVoice::default(); NUM_VOICES],
        }
    }
}

fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Lane {
    /// Smooths the knobs and rederives the voicing and registration when
    /// they move; returns the registration's key.
    #[inline(always)]
    fn control(&mut self, knobs: [f32; 3], state: &MiChordsState) -> u32 {
        for (held, value) in self.held.iter_mut().zip(knobs) {
            if value.is_finite() {
                *held = value;
            }
        }
        let targets = [knob(self.held[1]), knob(self.held[2])];
        if self.started {
            for (smoothed, target) in self.smoothed.iter_mut().zip(targets) {
                *smoothed += state.smoothing * (target - *smoothed);
                // Settled: lands on the target rather than creeping toward
                // it through denormals.
                if (target - *smoothed).abs() < 1e-6 {
                    *smoothed = target;
                }
            }
        } else {
            self.smoothed = targets;
            self.started = true;
        }
        let [inversion, waveform] = self.smoothed;

        self.bank.set_chord(self.held[0]);
        let key = (self.bank.chord_index(), inversion.to_bits());
        if self.inversion_key != Some(key) {
            self.inversion = self.bank.inversion(inversion);
            self.inversion_key = Some(key);
        }
        let position = (1.0 - waveform * 2.15).max(0.0);
        let key = position.to_bits();
        if self.registration_key != Some(key) {
            self.registration = registration(&REGISTRATIONS, position);
            self.registration_key = Some(key);
        }
        key
    }

    /// One sample of the chord and of its root alone, with the root at `f0`
    /// cycles per sample at no inversion, before the output gain.
    #[inline(always)]
    fn step(&mut self, f0: f32, knobs: [f32; 3], state: &MiChordsState) -> (f32, f32) {
        let registration_key = self.control(knobs, state);
        let [_, waveform] = self.smoothed;
        // Plaits' chords sound two octaves below its note at no inversion.
        let f0 = limit_frequency(f0 * 4.0, 2.0) * 0.998;
        let scan = ((waveform - 0.535) * 2.15).clamp(0.0, 1.0);
        let mut out = 0.0;
        let mut root = 0.0;
        for (voice, (divide_down, wavetable)) in self
            .divide_down
            .iter_mut()
            .zip(&mut self.wavetable)
            .enumerate()
        {
            let wavetable_amount = (50.0 * (waveform - FADE_POINTS[voice])).clamp(0.0, 1.0);
            let note_f0 = f0 * self.inversion.ratios[voice];
            let divide_down_amount = (1.0 - wavetable_amount) * fade(note_f0, state.to_48k);
            let amplitude = self.inversion.amplitudes[voice];
            let destination = if self.inversion.root_mask & (1 << voice) != 0 {
                &mut root
            } else {
                &mut out
            };
            if wavetable_amount != 0.0 {
                let frequency = limit_frequency(note_f0 * 1.004, state.max_frequency);
                // Plaits holds the pitch just above zero, so the wave never
                // stops.
                let frequency = if frequency.abs() < 1e-7 {
                    1e-7
                } else {
                    frequency
                };
                *destination += wavetable.step(
                    frequency,
                    frequency * state.to_48k,
                    amplitude * wavetable_amount,
                    scan,
                );
            }
            if divide_down_amount != 0.0 {
                *destination += divide_down.step(
                    note_f0,
                    state.to_48k,
                    &self.registration,
                    registration_key,
                    amplitude * divide_down_amount,
                );
            }
        }
        (out + root, root * 3.0)
    }
}

impl MiChords {
    fn init(&mut self, sample_rate: f32) {
        self.state = MiChordsState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        for (lane, voice) in self.lane_state[..lanes].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(lane),
                p.fm.value_or(lane, MiChordsParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = [
                p.chord.value_or(lane, MiChordsParams::CHORD_DEFAULT),
                p.inversion
                    .value_or(lane, MiChordsParams::INVERSION_DEFAULT),
                p.waveform.value_or(lane, MiChordsParams::WAVEFORM_DEFAULT),
            ];
            let (chord, root) = voice.step(f0, knobs, &self.state);
            self.outputs.chord.set(lane, OUTPUT_GAIN * chord);
            self.outputs.root.set(lane, OUTPUT_GAIN * root);
        }
    }
}

message_handlers!(impl MiChords {});

#[cfg(test)]
mod tests;
