mod arpeggiator;
mod nes_triangle;
mod super_square;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_docs::{DocExample, DocSection, DocTable, ModuleDocs};
use crate::reference_category::ReferenceCategory;
use crate::{
    dsp::{
        mi::{
            chords::CHORD_TABLE,
            core::{
                blend::Blend,
                chords::{ChordBank, Inversion, NUM_NOTES, NUM_VOICES, quantize},
                phase::{limit_frequency, max_frequency},
                random::Random,
                units::semitones_to_ratio,
            },
        },
        oscillators::{FmMode, apply_fm},
        utils::{
            SchmittTrigger,
            rng::{hash_bipolar, seed_base},
        },
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
};

use arpeggiator::{Arpeggiator, Mode};
use nes_triangle::NesTriangle;
use super_square::SuperSquare;

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.5 * 5.0;

/// The arpeggio's patterns: four directions, each over one, two or four
/// octaves.
const NUM_PATTERNS: usize = 12;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MiChiptuneParams {
    #[input(
        "the root: the chord is built on it, the arpeggio climbs from it and the bass sounds an octave below",
        type = pitch
    )]
    freq: PolySignal,
    #[input(
        "steps the arpeggio to its next note; once patched, the `chord` output is silent until the first trigger. Unpatched, the whole chord sounds",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,
    #[input(
        "the chord, numbered as in the Chords table; a voltage between two holds the current chord until it passes the halfway mark. While `trigger` is patched it changes on the next trigger",
        type = control,
        default = 10.0,
        range = (0.0, 10.0),
    )]
    #[deserr(default)]
    chord: Option<PolySignal>,
    #[input(
        "the chord's voicing while `trigger` is unpatched: each volt lifts the four notes an octave, one after another, from close position (0) up through five rotations",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    inversion: Option<PolySignal>,
    #[input(
        "the arpeggio's pattern, numbered as in the Patterns table; it changes on the next trigger",
        type = control,
        default = 0.0,
        range = (0.0, 11.0),
    )]
    #[deserr(default)]
    pattern: Option<PolySignal>,
    #[input(
        "the squares' tone: a thin pulse (0) widening to a square (2.5), then hard sync rising to a nasal buzz",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    shape: Option<PolySignal>,
    #[input(
        "how long each arpeggio note rings, from a blip (0) to held until the next (5); unused while `trigger` is unpatched",
        type = control,
        default = 5.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input("whether the bass fades with each arpeggio note too, or holds")]
    #[serde(default)]
    #[deserr(default)]
    bass_decay: bool,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero reflects frequencies below zero back up",
        type = audio,
    )]
    #[deserr(default)]
    fm: Option<PolySignal>,
    #[input("how `fm` bends the frequency")]
    #[serde(default)]
    #[deserr(default)]
    fm_mode: FmMode,
    #[input(
        "crossfades `_blend` from `chord` into `bass`",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    mix: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MiChiptuneOutputs {
    #[output(
        "blend",
        "`chord` crossfaded into `bass` by `mix`",
        type = cv,
        default,
        range = (-5.0, 5.0),
    )]
    blend: PolyOutput,
    #[output(
        "chord",
        "the chord, or the arpeggio while `trigger` is patched",
        type = cv,
        range = (-5.0, 5.0),
    )]
    chord: PolyOutput,
    #[output(
        "bass",
        "a stepped triangle an octave below `freq`, following the arpeggio's octave",
        type = cv,
        range = (-5.0, 5.0),
    )]
    bass: PolyOutput,
}

const MI_CHIPTUNE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Chiptune chords and arpeggios",
    description: "Mutable Instruments Plaits' chiptune engine: chords of \
                  square waves over a stepped triangle bass, or, with \
                  `trigger` patched, an arpeggio through the chord, a note \
                  per trigger.",
    categories: &[ReferenceCategory::Oscillator],
    sections: &[
        DocSection {
            title: "Chords",
            body: "`chord` picks one of eleven chords by number. The \
                   arpeggio plays each note once, leaving out notes that \
                   only double another.",
            table: Some(CHORD_TABLE),
            examples: &[],
        },
        DocSection {
            title: "Patterns",
            body: "`pattern` picks the arpeggio's direction and how many \
                   octaves it climbs.",
            table: Some(DocTable {
                columns: &["pattern", "Direction", "Octaves"],
                rows: &[
                    &["0", "up", "1"],
                    &["1", "up", "2"],
                    &["2", "up", "4"],
                    &["3", "down", "1"],
                    &["4", "down", "2"],
                    &["5", "down", "4"],
                    &["6", "up and down", "1"],
                    &["7", "up and down", "2"],
                    &["8", "up and down", "4"],
                    &["9", "random", "1"],
                    &["10", "random", "2"],
                    &["11", "random", "4"],
                ],
            }),
            examples: &[],
        },
    ],
    examples: &[
        DocExample {
            title: "Square-wave chord",
            code: "chip: $mi.chiptune('c3', { chord: 9 })",
        },
        DocExample {
            title: "Arpeggio",
            code: r#"
                arp: $mi.chiptune('a2', {
                  trigger: $pulse('8hz'),
                  chord: 5,
                  pattern: 7,
                  decay: 3,
                })
            "#,
        },
        DocExample {
            title: "Chord and bass apart",
            code: r#"
                const chip = $mi.chiptune('e3', { chord: 3, shape: 1 })
                chords: chip
                bass: chip._bass
            "#,
        },
    ],
};

#[module(
    name = "$mi.chiptune",
    docs = MI_CHIPTUNE_DOCS,
    args(freq),
    has_init,
    patch_update
)]
pub struct MiChiptune {
    outputs: MiChiptuneOutputs,
    state: MiChiptuneState,
    lane_state: Box<[Lane]>,
    params: MiChiptuneParams,
}

#[derive(Default)]
struct MiChiptuneState {
    inv_sample_rate: f32,
    /// Plaits' `kMaxFrequency` at this rate, at most half a cycle per sample
    /// so a phase wraps once a sample.
    max_frequency: f32,
    /// The envelope's per-sample decay is stated at 48 kHz; raised to this
    /// power it decays as fast at this rate.
    decay_exponent: f64,
    /// Plaits' one-pole (0.01 per sample at 48 kHz) easing the bass in and
    /// out of the envelope.
    bass_smoothing: f32,
}

impl MiChiptuneState {
    fn new(sample_rate: f32) -> Self {
        let decay_exponent = 48000.0 / f64::from(sample_rate);
        Self {
            inv_sample_rate: 1.0 / sample_rate,
            max_frequency: max_frequency(sample_rate).min(0.5),
            decay_exponent,
            bass_smoothing: (1.0 - 0.99f64.powf(decay_exponent)) as f32,
        }
    }
}

/// The knobs, in volts: chord, inversion, pattern, shape and decay.
type Knobs = [f32; 5];

#[derive(Clone, Copy)]
struct Lane {
    blend: Blend,
    bank: ChordBank,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: Knobs,
    inversion: Inversion,
    /// The chord and inversion knob, as bits, `inversion` was computed for.
    inversion_key: Option<(usize, u32)>,
    /// The chord's notes in the order the arpeggio climbs them, taken on each
    /// trigger.
    sorted: [f32; NUM_NOTES],
    /// The pattern the selector holds.
    pattern: usize,
    arpeggiator: Arpeggiator,
    random: Random,
    /// Whether `random` has its per-instance seed. Transfers with the lane,
    /// so a patch update never reseeds it.
    seeded: bool,
    trigger: SchmittTrigger,
    voices: [SuperSquare; NUM_VOICES],
    bass: NesTriangle,
    /// The arpeggio's envelope, struck to 1 by each trigger.
    envelope: f32,
    /// How far the bass follows the envelope (0–1).
    bass_envelope: f32,
    /// The envelope's per-sample decay, and the decay shape, as bits, it was
    /// computed for.
    decay: f32,
    decay_key: Option<u32>,
}

impl Default for Lane {
    fn default() -> Self {
        let bank = ChordBank::default();
        Self {
            blend: Blend::default(),
            bank,
            held: [
                MiChiptuneParams::CHORD_DEFAULT,
                MiChiptuneParams::INVERSION_DEFAULT,
                MiChiptuneParams::PATTERN_DEFAULT,
                MiChiptuneParams::SHAPE_DEFAULT,
                MiChiptuneParams::DECAY_DEFAULT,
            ],
            inversion: Inversion::default(),
            inversion_key: None,
            sorted: bank.sorted_ratios(),
            pattern: 0,
            arpeggiator: Arpeggiator::default(),
            random: Random::new(0),
            seeded: false,
            trigger: SchmittTrigger::default(),
            voices: [SuperSquare::default(); NUM_VOICES],
            bass: NesTriangle::default(),
            envelope: 0.0,
            bass_envelope: 0.0,
            decay: 1.0,
            decay_key: None,
        }
    }
}

/// A new side keeps its own `random`, so the sides' random arpeggios stay
/// independent.
impl crate::types::SeedSide for Lane {
    fn seed_side(&mut self, left: &Self) {
        let random = self.random;
        let seeded = self.seeded;
        *self = *left;
        self.random = random;
        self.seeded = seeded;
    }
}

fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

/// What the trigger does this sample: unpatched, or patched and rising or not.
#[derive(Clone, Copy, PartialEq)]
enum Clock {
    Unpatched,
    Patched { rising: bool },
}

impl Lane {
    /// One sample of the chord or arpeggio and of the bass, with `freq` at
    /// `f0` cycles per sample, before the output gain.
    #[inline(always)]
    fn step(
        &mut self,
        f0: f32,
        knobs: Knobs,
        clock: Clock,
        bass_decay: bool,
        state: &MiChiptuneState,
    ) -> (f32, f32) {
        for (held, value) in self.held.iter_mut().zip(knobs) {
            if value.is_finite() {
                *held = value;
            }
        }
        let [chord, inversion, pattern, shape, decay] = self.held;
        let shape = knob(shape) * 0.995;
        let max = state.max_frequency;
        // The voices run forward only: a frequency below zero is reflected.
        let f0 = limit_frequency(f0, 4.0).abs();

        let (mut out, root_transposition) = match clock {
            Clock::Patched { rising } => {
                if rising {
                    self.bank.set_chord(chord);
                    self.sorted = self.bank.sorted_ratios();
                    self.pattern = quantize(self.pattern, pattern, NUM_PATTERNS);
                    self.arpeggiator.mode = Mode::from_index(self.pattern / 3);
                    self.arpeggiator.range = 1 << (self.pattern % 3);
                    self.arpeggiator
                        .clock(self.bank.num_notes(), &mut self.random);
                    self.envelope = 1.0;
                }
                let octave = (1u32 << self.arpeggiator.octave()) as f32;
                let note_f0 = f0 * self.sorted[self.arpeggiator.note()] * octave;
                (self.voices[0].step(note_f0, shape, max), octave)
            }
            Clock::Unpatched => {
                self.bank.set_chord(chord);
                let inversion = knob(inversion);
                let key = (self.bank.chord_index(), inversion.to_bits());
                if self.inversion_key != Some(key) {
                    self.inversion = self.bank.inversion(inversion);
                    // Plaits inverts every other voice.
                    for amplitude in self.inversion.amplitudes.iter_mut().skip(1).step_by(2) {
                        *amplitude = -*amplitude;
                    }
                    self.inversion_key = Some(key);
                }
                // The voicing sounds two octaves below the note it is given,
                // so close position stands on `freq`.
                let chord_f0 = f0 * 4.0;
                let mut out = 0.0;
                for (voice, (ratio, amplitude)) in self
                    .voices
                    .iter_mut()
                    .zip(self.inversion.ratios.iter().zip(&self.inversion.amplitudes))
                {
                    out += voice.step(chord_f0 * ratio, shape, max) * amplitude;
                }
                (out, 1.0)
            }
        };

        let mut bass = self.bass.step(f0 * 0.5 * root_transposition);

        if clock != Clock::Unpatched {
            let shape = 1.0 - knob(decay);
            let key = shape.to_bits();
            if self.decay_key != Some(key) {
                let decay = 1.0 - 2.0 / 48000.0 * semitones_to_ratio(60.0 * shape) * shape;
                self.decay = f64::from(decay).powf(state.decay_exponent) as f32;
                self.decay_key = Some(key);
            }
            let bass_target = if bass_decay {
                (shape * 20.0).min(1.0)
            } else {
                0.0
            };
            self.bass_envelope += state.bass_smoothing * (bass_target - self.bass_envelope);
            self.envelope *= self.decay;
            out *= self.envelope;
            bass *= 1.0 + self.bass_envelope * (self.envelope - 1.0);
        }
        (out, bass)
    }
}

impl MiChiptune {
    fn init(&mut self, sample_rate: f32) {
        self.state = MiChiptuneState::new(sample_rate);
    }

    fn update(&mut self, sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        for (lane_index, lane) in self.lane_state[..lanes].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(lane_index),
                p.fm.value_or(lane_index, MiChiptuneParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let clock = match &p.trigger {
                Some(trigger) => Clock::Patched {
                    rising: lane.trigger.process(trigger.get_value(lane_index)),
                },
                None => Clock::Unpatched,
            };
            let knobs = [
                p.chord
                    .value_or(lane_index, MiChiptuneParams::CHORD_DEFAULT),
                p.inversion
                    .value_or(lane_index, MiChiptuneParams::INVERSION_DEFAULT),
                p.pattern
                    .value_or(lane_index, MiChiptuneParams::PATTERN_DEFAULT),
                p.shape
                    .value_or(lane_index, MiChiptuneParams::SHAPE_DEFAULT),
                p.decay
                    .value_or(lane_index, MiChiptuneParams::DECAY_DEFAULT),
            ];
            let (chord, bass) = lane.step(f0, knobs, clock, p.bass_decay, &self.state);
            // Plaits clips at full scale.
            let main = (OUTPUT_GAIN * chord).clamp(-5.0, 5.0);
            let aux = (OUTPUT_GAIN * bass).clamp(-5.0, 5.0);
            self.outputs.chord.set(lane_index, main);
            self.outputs.bass.set(lane_index, aux);
            let mix = (
                p.mix.value_or(lane_index, MiChiptuneParams::MIX_DEFAULT),
                p.mix.glide_mark(lane_index),
            );
            self.outputs
                .blend
                .set(lane_index, lane.blend.step(main, aux, mix, sample_rate));
        }
    }
}

impl crate::types::PatchUpdateHandler for MiChiptune {
    /// Seeds the random arpeggios of lanes that have none yet. Runs here, not
    /// in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same notes.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (lane_index, lane) in self.lane_state.iter_mut().enumerate() {
            if !lane.seeded {
                let unit = (hash_bipolar(base, lane_index as u64) + 1.0) * 0.5;
                lane.random = Random::new((unit * f64::from(u32::MAX)) as u32);
                lane.seeded = true;
            }
        }
    }
}

message_handlers!(impl MiChiptune {});

#[cfg(test)]
mod tests;
