use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::SchmittTrigger;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{LaneSum, PORT_MAX_LANES, PolyOutput, PolySignal};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct InversionParams {
    #[input("the notes to invert, one per lane, in any order", type = pitch)]
    input: PolySignal,
    #[input(
        "how many of the lowest notes are raised an octave, gliding through the inversions as it moves; negative drops the highest notes instead",
        type = control,
        control_range = (-5.0, 5.0),
    )]
    amount: LaneSum,
    #[input(
        "turns notes on and off: input lane N is inverted only while gate lane N is high; with every gate low the output holds",
        type = gate,
    )]
    #[deserr(default)]
    gate: Option<PolySignal>,
}

/// The notes `input` carries, leaving room for the voice a note in motion
/// fades in on. An input that already fills every lane inverts without its top
/// lane.
fn note_count(params: &InversionParams) -> usize {
    params.input.lanes().clamp(1, PORT_MAX_LANES - 1)
}

/// One lane per input note and one more: the voice a note in motion fades in
/// on.
#[allow(private_interfaces)]
pub fn inversion_derive_lane_count(params: &InversionParams) -> usize {
    note_count(params) + 1
}

/// Plaits' `ChordBank::ComputeChordInversion` for any number of notes: the
/// pitch and level (0–1) of each of `notes.len() + 1` voices with
/// `inversion` notes raised an octave (lowered, when negative). The whole
/// part rotates the notes across the voices; the fraction crossfades the note
/// in motion from its voice to the next, an octave up. As the inversion moves
/// continuously, a voice changes pitch only while silent, so nothing it drives
/// glides or clicks.
pub(super) fn invert(notes: &[f32], inversion: f32, pitches: &mut [f32], levels: &mut [f32]) {
    let n = notes.len() as i32;
    let voices = n + 1;
    let span = (n * voices) as f32;
    let inversion = inversion.clamp(-span, span);
    let integral = inversion.floor();
    let fractional = inversion - integral;
    let integral = integral as i32;
    let rotations = integral.div_euclid(n);
    let rotated = integral.rem_euclid(n);
    for (i, &note) in notes.iter().enumerate() {
        let i = i as i32;
        let octaves = (n - 1 + integral - i).div_euclid(n);
        let pitch = note + octaves as f32;
        let target = (i - rotations).rem_euclid(voices) as usize;
        let previous = (i - rotations - 1).rem_euclid(voices) as usize;
        if i == rotated {
            pitches[target] = pitch;
            levels[target] = 1.0 - fractional;
            // As the next whole inversion computes it, so the pitch holds
            // exactly when the note arrives.
            pitches[previous] = note + (octaves + 1) as f32;
            levels[previous] = fractional;
        } else if i < rotated {
            pitches[previous] = pitch;
            levels[previous] = 1.0;
        } else {
            pitches[target] = pitch;
            levels[target] = 1.0;
        }
    }
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct InversionOutputs {
    #[output(
        "pitch",
        "the inverted notes, one lane more than the input, the notes rotating across them",
        type = cv,
        default
    )]
    output: PolyOutput,
    #[output(
        "level",
        "each lane's level, full unless a note is fading between octaves",
        type = cv,
        range = (0.0, 5.0),
    )]
    level: PolyOutput,
}

const INVERSION_DOCS: ModuleDocs = ModuleDocs {
    summary: "Glide notes through inversions",
    description: "Glides a chord through its inversions, crossfading each moving note \
                  between its two octaves so nothing clicks. It outputs one lane more than \
                  its input: scale each voice by `level`, or the extra lane sounds too.",
    categories: &[ModuleCategory::Pitch],
    sections: &[],
    examples: &[
        DocExample {
            title: "Second inversion",
            code: r#"
                const c = $inversion(['c3', 'e3', 'g3'], 2)
                chord: $saw(c).amp(c._level)
            "#,
        },
        DocExample {
            title: "Gliding through the inversions",
            code: r#"
                const c = $inversion(
                  ['c3', 'e3', 'g3', 'b3'],
                  $sine('0.05hz').range(0, 4),
                )
                chord: $saw(c).amp(c._level)
            "#,
        },
        DocExample {
            title: "Inverting the keys you hold",
            code: r#"
                const midi = $midiCV({ lanes: 4 })
                const c = $inversion(midi, $sine('0.05hz').range(0, 4), {
                  gate: midi._gate,
                })
                chord: $saw(c).amp(c._level)
            "#,
        },
    ],
};

#[module(name = "$inversion", stereo = mono, docs = INVERSION_DOCS, lanes_derive = inversion_derive_lane_count, args(input, amount), experimental)]
pub struct Inversion {
    outputs: InversionOutputs,
    params: InversionParams,
    state: InversionState,
}

struct InversionState {
    /// The last finite amount, which a NaN or infinite one holds; `None`
    /// until the first.
    amount: Option<f32>,
    /// One per input lane, reading the matching gate lane.
    gate_triggers: [SchmittTrigger; PORT_MAX_LANES],
    /// The voices last written, which hold while every gate is low. Levels
    /// are 0–1.
    pitches: [f32; PORT_MAX_LANES],
    levels: [f32; PORT_MAX_LANES],
}

impl Default for InversionState {
    fn default() -> Self {
        Self {
            amount: None,
            gate_triggers: [SchmittTrigger::default(); PORT_MAX_LANES],
            pitches: [0.0; PORT_MAX_LANES],
            levels: [0.0; PORT_MAX_LANES],
        }
    }
}

impl Inversion {
    fn update(&mut self, _sample_rate: f32) {
        let notes_len = note_count(&self.params);
        let gate = self.params.gate.as_ref();
        let mut notes = [0.0; PORT_MAX_LANES];
        let mut active = 0;
        for lane in 0..notes_len {
            let on = match gate {
                Some(gate) => {
                    self.state.gate_triggers[lane]
                        .process_with_edge(gate.get_value(lane))
                        .0
                }
                None => true,
            };
            if on {
                notes[active] = self.params.input.get_value(lane);
                active += 1;
            }
        }
        // Inverting raises the lowest notes, whatever lanes they arrive on.
        // Two notes swap places only as they cross, at the same pitch.
        notes[..active].sort_unstable_by(f32::total_cmp);
        let value = self.params.amount.get_value();
        if value.is_finite() {
            self.state.amount = Some(value);
        }
        if let (true, Some(held)) = (active > 0, self.state.amount) {
            // Voices past the active notes fall silent and keep their pitch.
            self.state.levels = [0.0; PORT_MAX_LANES];
            invert(
                &notes[..active],
                held,
                &mut self.state.pitches[..active + 1],
                &mut self.state.levels[..active + 1],
            );
        }
        for lane in 0..notes_len + 1 {
            self.outputs.output.set(lane, self.state.pitches[lane]);
            self.outputs.level.set(lane, 5.0 * self.state.levels[lane]);
        }
    }
}

message_handlers!(impl Inversion {});

#[cfg(test)]
#[path = "tests_inversion.rs"]
mod tests;
