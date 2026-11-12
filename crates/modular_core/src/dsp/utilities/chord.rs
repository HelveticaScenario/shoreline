use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, DocSection, DocTable, ModuleDocs};
use crate::poly::{MonoSignal, MonoSignalExt, PORT_MAX_CHANNELS, PolyOutput};

use super::scale::ScaleSpec;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ChordParams {
    #[input("scale or chord spec, e.g. \"C(major)\", \"Eb3[m7]\", \"C[just maj7 inv1]\"")]
    chord: ScaleSpec,
    #[input(
        "how many of the lowest notes are raised an octave, gliding through the inversions as it moves; negative drops the highest notes instead",
        type = control,
        default = 0.0,
        control_range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    inversion: Option<MonoSignal>,
}

/// One channel per note of the spec, and one more when `inversion` is
/// patched: the voice a note in motion fades in on. A spec that already fills
/// every channel keeps its count and inverts without its top note.
#[allow(private_interfaces)]
pub fn chord_derive_channel_count(params: &ChordParams) -> usize {
    let notes = params.chord.voltages().len();
    if params.inversion.is_some() {
        (notes + 1).min(PORT_MAX_CHANNELS)
    } else {
        notes
    }
}

/// Plaits' `ChordBank::ComputeChordInversion` for any number of notes: the
/// pitch and level (0–1) of each of `notes.len() + 1` voices with
/// `inversion` notes raised an octave (lowered, when negative). The whole
/// part rotates the notes across the voices; the fraction crossfades the note
/// in motion from its voice to the next, an octave up. As the inversion moves
/// continuously, a voice changes pitch only while silent, so nothing it drives
/// glides or clicks.
fn invert(notes: &[f32], inversion: f32, pitches: &mut [f32], levels: &mut [f32]) {
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
struct ChordOutputs {
    #[output(
        "pitch",
        "one pitch per note, lowest first; with `inversion` patched, one more channel, the notes rotating across them",
        type = cv,
        default
    )]
    output: PolyOutput,
    #[output(
        "level",
        "each channel's level, full (5) unless `inversion` is fading a note between octaves",
        type = cv,
        range = (0.0, 5.0),
    )]
    level: PolyOutput,
}

const CHORD_DOCS: ModuleDocs = ModuleDocs {
    summary: "Notes of a chord or scale",
    description: "Outputs every note of a chord or scale as pitch voltages, one channel per \
                  note, lowest first. Patch it into an oscillator to hear the whole chord, \
                  or into `$quantizer` to keep a melody inside it. Patching `inversion` adds \
                  a channel, and the moving note crossfades between two of them: scale each \
                  voice by `level`, or the extra channel sounds too.",
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
            code: "$saw($chord('C3[maj7]')).lpf('c5', 1).out()",
        },
        DocExample {
            title: "Gliding through the inversions",
            code: r#"
                const inv = $sine('0.05hz').range(0, 4)
                const c = $chord('C3[maj7]', { inversion: inv })
                $saw(c).amp(c._level).out()
            "#,
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
    state: ChordState,
}

#[derive(Default)]
struct ChordState {
    /// The last finite inversion, which a NaN or infinite one holds; `None`
    /// until the first.
    inversion: Option<f32>,
}

impl Chord {
    fn update(&mut self, _sample_rate: f32) {
        let notes = self.params.chord.voltages();
        match &self.params.inversion {
            None => {
                for (ch, &v) in notes.iter().enumerate() {
                    self.outputs.output.set(ch, v);
                    self.outputs.level.set(ch, 5.0);
                }
            }
            Some(inversion) => {
                let voices = (notes.len() + 1).min(PORT_MAX_CHANNELS);
                let value = inversion.get_value();
                if value.is_finite() {
                    self.state.inversion = Some(value);
                }
                let mut pitches = [0.0; PORT_MAX_CHANNELS];
                let mut levels = [0.0; PORT_MAX_CHANNELS];
                if let Some(held) = self.state.inversion {
                    invert(
                        &notes[..voices - 1],
                        held,
                        &mut pitches[..voices],
                        &mut levels[..voices],
                    );
                }
                for ch in 0..voices {
                    self.outputs.output.set(ch, pitches[ch]);
                    self.outputs.level.set(ch, 5.0 * levels[ch]);
                }
            }
        }
    }
}

message_handlers!(impl Chord {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly::PolySignal;
    use crate::types::{OutputStruct, Signal};

    fn make_chord(spec: &str) -> Chord {
        make_inverted(spec, None)
    }

    fn make_inverted(spec: &str, inversion: Option<f32>) -> Chord {
        let params = ChordParams {
            chord: ScaleSpec::parse(spec).unwrap(),
            inversion: inversion.map(|v| MonoSignal::from_poly(PolySignal::mono(Signal::Volts(v)))),
        };
        let channels = chord_derive_channel_count(&params);
        let mut outputs = ChordOutputs::default();
        outputs.set_all_channels(channels);
        Chord {
            params,
            outputs,
            state: ChordState::default(),
            _channel_count: channels,
            _block_index: Default::default(),
        }
    }

    /// The voices of `spec` at `inversion`: each channel's pitch and level
    /// (0–1).
    fn voices(spec: &str, inversion: f32) -> Vec<(f32, f32)> {
        let mut chord = make_inverted(spec, Some(inversion));
        chord.update(48000.0);
        (0..chord.channel_count())
            .map(|ch| {
                (
                    chord.outputs.output.get(ch),
                    chord.outputs.level.get(ch) / 5.0,
                )
            })
            .collect()
    }

    /// The pitches sounding at full level, lowest first.
    fn sounding(voices: &[(f32, f32)]) -> Vec<f32> {
        let mut pitches: Vec<f32> = voices
            .iter()
            .filter(|(_, level)| *level == 1.0)
            .map(|(pitch, _)| *pitch)
            .collect();
        pitches.sort_by(f32::total_cmp);
        pitches
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

    #[test]
    fn unpatched_inversion_leaves_the_chord_at_full_level() {
        let mut chord = make_chord("C3[maj7]");
        chord.update(48000.0);
        for ch in 0..4 {
            assert_eq!(chord.outputs.level.get(ch), 5.0, "ch {ch}");
        }
    }

    /// A whole inversion sounds the notes the spec's `invN` gives, on one
    /// more channel than the chord has notes.
    #[test]
    fn whole_inversions_raise_the_lowest_notes() {
        for spec in ["C3[maj7]", "D3[m9]", "F[sus4]"] {
            let notes = ScaleSpec::parse(spec).unwrap().voltages().len();
            for k in 0..notes {
                let v = voices(spec, k as f32);
                assert_eq!(v.len(), notes + 1, "{spec}");
                let expected = if k == 0 {
                    ScaleSpec::parse(spec).unwrap().voltages().to_vec()
                } else {
                    let inverted = spec.replace(']', &format!(" inv{k}]"));
                    let mut v = ScaleSpec::parse(&inverted).unwrap().voltages().to_vec();
                    v.sort_by(f32::total_cmp);
                    v
                };
                let actual = sounding(&v);
                assert_eq!(actual.len(), notes, "{spec} inv {k}");
                for (a, e) in actual.iter().zip(&expected) {
                    assert!(
                        (a - e).abs() < 1e-5,
                        "{spec} inv {k}: {actual:?} vs {expected:?}"
                    );
                }
            }
        }
    }

    /// A negative inversion drops the highest notes an octave.
    #[test]
    fn negative_inversions_drop_the_highest_notes() {
        let notes = ScaleSpec::parse("C3[maj7]").unwrap().voltages().to_vec();
        let actual = sounding(&voices("C3[maj7]", -1.0));
        let mut expected = vec![notes[3] - 1.0, notes[0], notes[1], notes[2]];
        expected.sort_by(f32::total_cmp);
        assert_eq!(actual, expected);
    }

    /// Between whole inversions the moving note crossfades between its two
    /// octaves, and the levels always sum to the number of notes.
    #[test]
    fn fractions_crossfade_the_moving_note() {
        let notes = ScaleSpec::parse("C3[maj7]").unwrap().voltages().to_vec();
        let v = voices("C3[maj7]", 1.25);
        let total: f32 = v.iter().map(|(_, level)| level).sum();
        assert!((total - 4.0).abs() < 1e-6, "{v:?}");
        let low = v.iter().find(|(p, _)| (p - notes[1]).abs() < 1e-6).unwrap();
        let high = v
            .iter()
            .find(|(p, _)| (p - (notes[1] + 1.0)).abs() < 1e-6)
            .unwrap();
        assert!(
            (low.1 - 0.75).abs() < 1e-6 && (high.1 - 0.25).abs() < 1e-6,
            "{v:?}"
        );
    }

    /// Swept across its whole span, each voice changes pitch only while it is
    /// silent (to within the sweep's step), so an oscillator it drives never
    /// glides or clicks audibly.
    #[test]
    fn voices_move_only_while_silent() {
        for spec in ["C3[maj7]", "A2[m]", "C(major)"] {
            let notes = ScaleSpec::parse(spec).unwrap().voltages().len() as f32;
            let span = notes * (notes + 1.0);
            let step = 0.01;
            let mut previous = voices(spec, -span);
            let mut x = -span;
            while x < span {
                x += step;
                let current = voices(spec, x);
                for (ch, ((p0, l0), (p1, l1))) in previous.iter().zip(&current).enumerate() {
                    if p0 != p1 {
                        assert!(
                            *l0 <= 2.0 * step && *l1 <= 2.0 * step,
                            "{spec} at {x}: channel {ch} moved {p0} → {p1} at levels {l0}, {l1}"
                        );
                    }
                }
                previous = current;
            }
        }
    }

    fn read(chord: &Chord) -> Vec<(f32, f32)> {
        (0..chord.channel_count())
            .map(|ch| (chord.outputs.output.get(ch), chord.outputs.level.get(ch)))
            .collect()
    }

    /// A NaN or infinite inversion holds the voices, pitches and levels,
    /// where they are.
    #[test]
    fn non_finite_inversion_holds() {
        let mut chord = make_inverted("C3[maj7]", Some(1.5));
        chord.update(48000.0);
        let before = read(&chord);
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            chord.params.inversion =
                Some(MonoSignal::from_poly(PolySignal::mono(Signal::Volts(v))));
            chord.update(48000.0);
            assert_eq!(read(&chord), before, "{v}");
        }
    }

    /// Editing the spec while the inversion stays put plays the new chord at
    /// that inversion: the state a patch update carries over holds no chord.
    #[test]
    fn a_new_spec_sounds_after_a_patch_update() {
        for (from, to) in [
            ("C3[maj7]", "F3[m7]"),
            ("C3[maj]", "C3[maj7]"),
            ("C3[maj7]", "C3[maj]"),
        ] {
            let mut old = make_inverted(from, Some(1.0));
            old.update(48000.0);
            let mut new = make_inverted(to, Some(1.0));
            std::mem::swap(&mut new.state, &mut old.state);
            new.update(48000.0);
            assert_eq!(
                read(&new),
                {
                    let mut fresh = make_inverted(to, Some(1.0));
                    fresh.update(48000.0);
                    read(&fresh)
                },
                "{from} → {to}"
            );
        }
    }

    /// A whole rotation, `inversion` at the number of notes, is the chord an
    /// octave up; its negative, an octave down.
    #[test]
    fn whole_rotations_transpose_by_octaves() {
        for spec in ["C3[maj7]", "A2[m]", "C(major)"] {
            let notes = ScaleSpec::parse(spec).unwrap().voltages().to_vec();
            let n = notes.len() as f32;
            for (inversion, octaves) in [(n, 1.0), (-n, -1.0), (2.0 * n, 2.0)] {
                let mut expected: Vec<f32> = notes.iter().map(|v| v + octaves).collect();
                expected.sort_by(f32::total_cmp);
                let actual = sounding(&voices(spec, inversion));
                assert_eq!(actual.len(), notes.len(), "{spec} at {inversion}");
                for (a, e) in actual.iter().zip(&expected) {
                    assert!(
                        (a - e).abs() < 1e-5,
                        "{spec} at {inversion}: {actual:?} vs {expected:?}"
                    );
                }
            }
        }
    }

    /// Past ±notes × (notes + 1) the inversion holds the voicing at its end.
    #[test]
    fn inversion_holds_past_its_span() {
        let span = 4.0 * 5.0;
        assert_eq!(voices("C3[maj7]", span), voices("C3[maj7]", span + 7.3));
        assert_eq!(voices("C3[maj7]", -span), voices("C3[maj7]", -1e30));
    }

    /// One- and two-note chords invert too.
    #[test]
    fn small_chords_invert() {
        let single = voices("C[0]", 0.5);
        assert_eq!(single.len(), 2);
        assert_eq!(sounding(&voices("C[0]", 1.0)), vec![1.0]);
        let fifth = voices("C[0 7]", 1.0);
        assert_eq!(fifth.len(), 3);
        let sounding = sounding(&fifth);
        assert!(
            (sounding[0] - 7.0 / 12.0).abs() < 1e-6 && (sounding[1] - 1.0).abs() < 1e-6,
            "{sounding:?}"
        );
    }

    /// Patching `inversion` adds one channel, up to the port's limit.
    #[test]
    fn inversion_adds_a_channel() {
        assert_eq!(make_inverted("C3[maj7]", None).channel_count(), 4);
        assert_eq!(make_inverted("C3[maj7]", Some(0.0)).channel_count(), 5);
        assert_eq!(make_inverted("chromatic", Some(0.0)).channel_count(), 13);
    }
}
