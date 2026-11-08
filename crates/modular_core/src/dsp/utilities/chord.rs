use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, DocSection, DocTable, ModuleDocs};
use crate::poly::PolyOutput;

use super::scale::ScaleSpec;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ChordParams {
    #[input("scale or chord spec, e.g. \"C(major)\", \"Eb3[m7]\", \"C[just maj7 inv1]\"")]
    chord: ScaleSpec,
}

/// One channel per note of the spec.
#[allow(private_interfaces)]
pub fn chord_derive_channel_count(params: &ChordParams) -> usize {
    params.chord.voltages().len()
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ChordOutputs {
    #[output("output", "one pitch per note, lowest first", default)]
    output: PolyOutput,
}

const CHORD_DOCS: ModuleDocs = ModuleDocs {
    summary: "Notes of a chord or scale",
    description: "Outputs every note of a chord or scale as pitch voltages, one channel per \
                  note, lowest first. Patch it into an oscillator to hear the whole chord, \
                  or into `$quantizer` to keep a melody inside it.",
    categories: &[ModuleCategory::Pitch],
    sections: &[
        DocSection {
            title: "Writing a chord or scale",
            body: "A root note (octave optional, 4 if left out), then a scale in `()` or a \
                   chord in `[]`.",
            table: Some(DocTable {
                columns: &["Form", "Example", "Gives"],
                rows: &[
                    &[
                        "`root(scale)`",
                        "`'D3(dorian)'`",
                        "one octave of a named scale",
                    ],
                    &[
                        "`root(intervals)`",
                        "`'C(0 3 7 10)'`",
                        "semitones above the root, within an octave",
                    ],
                    &[
                        "`root(tuning)`",
                        "`'A(just)'`",
                        "all 12 steps in another tuning",
                    ],
                    &[
                        "`root[chord]`",
                        "`'F#2[m9]'`",
                        "the chord's notes; extensions stay up high",
                    ],
                    &[
                        "`root[intervals]`",
                        "`'C[0 7 16]'`",
                        "your own voicing, intervals kept as written",
                    ],
                    &[
                        "`root[tuning chord]`",
                        "`'C[just maj]'`",
                        "a chord in `just` or `pythag` tuning",
                    ],
                    &[
                        "`root[chord invN]`",
                        "`'C[maj7 inv1]'`",
                        "the lowest N notes raised an octave",
                    ],
                    &["`chromatic`", "`'chromatic'`", "all 12 notes from C4"],
                ],
            }),
            examples: &[],
        },
        DocSection {
            title: "Names",
            body: "",
            table: Some(DocTable {
                columns: &["Kind", "Names"],
                rows: &[
                    &[
                        "Scales",
                        "major, minor, dorian, phrygian, lydian, mixolydian, locrian, harmonic minor, melodic minor, pentatonic major, pentatonic minor, blues, whole tone, chromatic",
                    ],
                    &[
                        "Chords",
                        "maj / M, min / m, dim, aug, sus2, sus4, 5, 6, m6, 69, 7, maj7, m7, mMaj7, dim7, m7b5, aug7, maj7#5, 7sus4, 7sus2, 7b5, 9, maj9, m9, 11, maj11, m11, 13, maj13, m13, 7b9, 7#9, 7#11, add9, madd9, add11",
                    ],
                ],
            }),
            examples: &[],
        },
    ],
    examples: &[
        DocExample {
            title: "Warm maj7 pad",
            code: "$saw($chord('C3[maj7]')).$.lpf('c5', 1).out()",
        },
        DocExample {
            title: "A drifting melody that stays in key",
            code: r#"
                $sine(
                  $quantizer(
                    $sine('0.1hz').range('a3', 'a5'),
                    $chord('A(minor)'),
                  ),
                ).out()
            "#,
        },
    ],
};

#[module(name = "$chord", docs = CHORD_DOCS, channels_derive = chord_derive_channel_count, args(chord))]
pub struct Chord {
    outputs: ChordOutputs,
    params: ChordParams,
}

impl Chord {
    fn update(&mut self, _sample_rate: f32) {
        for (ch, &v) in self.params.chord.voltages().iter().enumerate() {
            self.outputs.output.set(ch, v);
        }
    }
}

message_handlers!(impl Chord {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OutputStruct;

    fn make_chord(spec: &str) -> Chord {
        let params = ChordParams {
            chord: ScaleSpec::parse(spec).unwrap(),
        };
        let channels = chord_derive_channel_count(&params);
        let mut outputs = ChordOutputs::default();
        outputs.set_all_channels(channels);
        Chord {
            params,
            outputs,
            _channel_count: channels,
            _block_index: Default::default(),
        }
    }

    #[test]
    fn outputs_one_channel_per_note() {
        let mut chord = make_chord("C3[maj7]");
        assert_eq!(chord.channel_count(), 4);
        chord.update(48000.0);
        let expected = [-12.0, -8.0, -5.0, -1.0];
        for (ch, semis) in expected.iter().enumerate() {
            let v = chord.outputs.output.get(ch);
            assert!((v - semis / 12.0).abs() < 1e-6, "ch {ch}: {v}");
        }
    }

    #[test]
    fn scales_output_every_degree() {
        assert_eq!(make_chord("C(major)").channel_count(), 7);
        assert_eq!(make_chord("chromatic").channel_count(), 12);
    }

    #[test]
    fn just_tuning_applies_to_chord_notes() {
        let mut chord = make_chord("C[just maj]");
        chord.update(48000.0);
        assert!((chord.outputs.output.get(1) as f64 - 1.25_f64.log2()).abs() < 1e-6);
    }
}
