use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::{
            chords::CHORDS_SECTION,
            core::{
                chords::{ChordBank, NUM_NOTES},
                ensemble::{Ensemble, EnsembleRates},
                filter::{NaiveSvf, naive_svf_coefficients},
                phase::limit_frequency,
                units::semitones_to_ratio,
            },
            divide_down::{
                REGISTRATIONS,
                oscillator::{DivideDownVoice, fade, registration},
            },
        },
        oscillators::{FmMode, apply_fm},
        utils::voct_to_hz,
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.8 * 5.0;
const FULL_SCALE: f32 = 5.0;
/// The lowest filter cutoff, 10 Hz in cycles per 48 kHz sample.
const CUTOFF_FLOOR: f32 = 10.0 / 48000.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct StringMachineParams {
    #[input("root pitch", type = pitch, fold)]
    freq: PolySignal,
    #[input(
        "the chord, numbered as in the Chords table; a voltage between two holds the current chord until it passes the halfway mark",
        type = control,
        default = 10.0,
        range = (0.0, 10.0),
        fold,
    )]
    #[deserr(default)]
    chord: Option<PolySignal>,
    #[input(
        "the mix of saw and square footages, from a bright saw (0) through organ-like mixtures to a hollow square (5)",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    registration: Option<PolySignal>,
    #[input(
        "the filter opens from dark (0) to bright (5), while the ensemble fades out toward 2.5 and back in, shallow below it and deep above",
        type = control,
        default = 3.5,
        range = (0.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    tone: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); the filter follows exp's pitch and the unmodulated note under lin and throughZero, which runs the saws backward",
        type = audio,
        fold,
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
struct StringMachineOutputs {
    #[output(
        "output",
        "the strings, as one stereo lane: alternate notes of the chord lead on each side",
        type = cv,
        default,
        range = (-5.0, 5.0),
    )]
    sample: PolyOutput,
}

const STRING_MACHINE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Stereo string-machine chords",
    description: "Mutable Instruments Plaits' string machine: four-note \
                  chords on divide-down organ voices, low-pass filtered and \
                  spread across a stereo ensemble chorus.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[CHORDS_SECTION],
    examples: &[
        DocExample {
            title: "Minor 7th strings",
            code: "pad: $mi.stringMachine('c3', { chord: 4 })",
        },
        DocExample {
            title: "Swelling ensemble",
            code: r#"
                const swell = $sine('0.07hz').range(2.5, 5)
                pad: $mi.stringMachine('a2', { chord: 6, tone: swell })
            "#,
        },
        DocExample {
            title: "Square organ chords",
            code: r#"
                keys: $mi.stringMachine('e3', {
                  chord: 9,
                  registration: 5,
                  tone: 1,
                })
            "#,
        },
    ],
};

#[module(
    name = "$mi.stringMachine", stereo = stereo,
    docs = STRING_MACHINE_DOCS,
    args(freq),
    has_init
)]
pub struct StringMachine {
    outputs: StringMachineOutputs,
    state: StringMachineState,
    lane_state: Box<[Lane]>,
    params: StringMachineParams,
}

#[derive(Default)]
struct StringMachineState {
    sample_rate: f32,
    inv_sample_rate: f32,
    to_48k: f32,
    /// Plaits' registration smoothing (0.1 per 12-sample block at 48 kHz) as
    /// a per-sample one-pole coefficient.
    smoothing: f32,
    rates: EnsembleRates,
}

impl StringMachineState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            smoothing: (1.0 - 0.9f64.powf(1.0 / 12.0 * 48000.0 / f64::from(sample_rate))) as f32,
            rates: EnsembleRates::new(sample_rate),
        }
    }
}

/// One voice of the string machine. A stereo lane's left side carries it;
/// its right side's entry stays empty.
#[derive(Default)]
struct Lane {
    bank: ChordBank,
    registration_glide: SwapGlide,
    tone_glide: SwapGlide,
    /// The last finite chord, registration and tone, which a NaN or infinite
    /// knob holds.
    held: [f32; 3],
    /// The registration (0–1) smoothed as Plaits smooths it.
    smoothed: f32,
    started: bool,
    /// The partial amplitudes of the smoothed registration.
    registration: [f32; 7],
    /// The smoothed registration `registration` was computed for, as bits.
    registration_key: Option<u32>,
    voices: [DivideDownVoice; NUM_NOTES],
    filters: [Filter; 2],
    ensemble: Ensemble,
}

/// One of the engine's two low-pass filters, with its coefficients cached on
/// the cutoff they were derived for.
#[derive(Default)]
struct Filter {
    filter: NaiveSvf,
    /// The cutoff (cycles per 48 kHz sample) the coefficients are for, as
    /// bits.
    key: Option<u32>,
}

impl Filter {
    #[inline(always)]
    fn set(&mut self, cutoff: f32, to_48k: f32) {
        if self.key != Some(cutoff.to_bits()) {
            self.filter
                .set_coefficients(naive_svf_coefficients(cutoff, 1.0, to_48k));
            self.key = Some(cutoff.to_bits());
        }
    }
}

impl Lane {
    fn new(rates: &EnsembleRates) -> Self {
        Self {
            held: [
                StringMachineParams::CHORD_DEFAULT,
                StringMachineParams::REGISTRATION_DEFAULT,
                StringMachineParams::TONE_DEFAULT,
            ],
            ensemble: Ensemble::new(rates),
            ..Default::default()
        }
    }
}

fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Lane {
    /// One stereo sample with the notes at `f0` cycles per sample and the
    /// filter tracking `cutoff_f0`, before the output gain. `knobs` are the
    /// chord, registration and tone in volts.
    #[inline(always)]
    fn step(
        &mut self,
        f0: f32,
        cutoff_f0: f32,
        knobs: [f32; 3],
        state: &StringMachineState,
    ) -> (f32, f32) {
        for (held, value) in self.held.iter_mut().zip(knobs) {
            if value.is_finite() {
                *held = value;
            }
        }
        let [chord, registration_volts, tone] = self.held;
        let target = knob(registration_volts);
        if self.started {
            self.smoothed += state.smoothing * (target - self.smoothed);
            // Settled: lands on the target rather than creeping toward it
            // through denormals.
            if (target - self.smoothed).abs() < 1e-6 {
                self.smoothed = target;
            }
        } else {
            self.smoothed = target;
            self.started = true;
        }
        let key = self.smoothed.to_bits();
        if self.registration_key != Some(key) {
            self.registration = registration(&REGISTRATIONS, self.smoothed);
            self.registration_key = Some(key);
        }
        self.bank.set_chord(chord);

        let f0 = limit_frequency(f0, 2.0) * 0.998;
        let mut sides = [0.0f32; 2];
        for (note, voice) in self.voices.iter_mut().enumerate() {
            let note_f0 = f0 * self.bank.ratio(note);
            let gain = 0.25 * fade(note_f0, state.to_48k);
            sides[note & 1] += voice.step(note_f0, state.to_48k, &self.registration, key, gain);
        }

        let timbre = knob(tone);
        // Plaits' cutoff in cycles per 48 kHz sample. The floor keeps the
        // filters following their input at a zero or NaN pitch, where a
        // zero cutoff would freeze them.
        let cutoff = (2.2
            * (limit_frequency(cutoff_f0, 0.5) * 0.998 * state.to_48k).abs()
            * semitones_to_ratio(120.0 * timbre))
        .max(CUTOFF_FLOOR);
        for (filter, cutoff) in self.filters.iter_mut().zip([cutoff, cutoff * 1.5]) {
            filter.set(cutoff, state.to_48k);
        }
        let l = self.filters[0].filter.low_pass(sides[0]);
        let r = self.filters[1].filter.low_pass(sides[1]);
        let left = 0.66 * l + 0.33 * r;
        let right = 0.66 * r + 0.33 * l;

        let amount = (timbre - 0.5).abs() * 2.0;
        let depth = 0.35 + 0.65 * timbre;
        self.ensemble
            .process(left, right, amount, depth, &state.rates)
    }
}

impl StringMachine {
    fn init(&mut self, sample_rate: f32) {
        self.state = StringMachineState::new(sample_rate);
        let channels = self.channels().max(1);
        for lane in self.lane_state.iter_mut().step_by(channels) {
            *lane = Lane::new(&self.state.rates);
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channels().max(1);
        let lanes = self.lane_count();
        let p = &self.params;
        let state = &self.state;
        for (lane, voice) in self.lane_state[..lanes]
            .iter_mut()
            .enumerate()
            .step_by(channels)
        {
            // One voice plays both sides, so it reads each input folded.
            let right = lane + channels - 1;
            let fold = |signal: &Option<PolySignal>, default: f32| {
                0.5 * (signal.value_or(lane, default) + signal.value_or(right, default))
            };
            let pitch = 0.5 * (p.freq.get_value(lane) + p.freq.get_value(right));
            let (f0, cutoff_f0) = match (&p.fm, p.fm_mode) {
                (None, _) => {
                    let f0 = voct_to_hz(pitch) * state.inv_sample_rate;
                    (f0, f0)
                }
                // Exponential FM moves the note, which the filter follows as
                // Plaits' does; linear FM leaves it on the unmodulated note.
                (Some(_), FmMode::Exp) => {
                    let f0 = apply_fm(pitch, fold(&p.fm, 0.0), FmMode::Exp) * state.inv_sample_rate;
                    (f0, f0)
                }
                (Some(_), mode) => (
                    apply_fm(pitch, fold(&p.fm, 0.0), mode) * state.inv_sample_rate,
                    voct_to_hz(pitch) * state.inv_sample_rate,
                ),
            };
            let registration = voice.registration_glide.follow(
                fold(&p.registration, StringMachineParams::REGISTRATION_DEFAULT),
                p.registration.glide_mark(lane),
                state.sample_rate,
            );
            let tone = voice.tone_glide.follow(
                fold(&p.tone, StringMachineParams::TONE_DEFAULT),
                p.tone.glide_mark(lane),
                state.sample_rate,
            );
            let knobs = [
                fold(&p.chord, StringMachineParams::CHORD_DEFAULT),
                registration,
                tone,
            ];
            let (left, right) = voice.step(f0, cutoff_f0, knobs, state);
            let out = |x: f32| (OUTPUT_GAIN * x).clamp(-FULL_SCALE, FULL_SCALE);
            self.outputs.sample.set(lane, out(left));
            if channels > 1 {
                self.outputs.sample.set(lane + 1, out(right));
            }
        }
    }
}

message_handlers!(impl StringMachine {});

#[cfg(test)]
mod tests;
