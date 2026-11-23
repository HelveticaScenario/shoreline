use super::*;
use crate::types::{OutputStruct, Signal};

fn semis(intervals: &[f32]) -> Vec<f32> {
    intervals.iter().map(|s| s / 12.0).collect()
}

/// C3 maj7.
fn maj7() -> Vec<f32> {
    semis(&[-12.0, -8.0, -5.0, -1.0])
}

/// Chords of one to seven notes.
fn chords() -> Vec<Vec<f32>> {
    vec![
        vec![0.0],
        semis(&[0.0, 7.0]),
        maj7(),
        semis(&[-15.0, -12.0, -8.0]),
        semis(&[0.0, 5.0, 7.0]),
        semis(&[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 11.0]),
    ]
}

fn pitches(notes: &[f32]) -> PolySignal {
    let signals: Vec<Signal> = notes.iter().map(|&v| Signal::Volts(v)).collect();
    PolySignal::poly(&signals)
}

fn make_inversion(notes: &[f32], amount: f32) -> Inversion {
    let params = InversionParams {
        input: pitches(notes),
        amount: LaneSum::from_poly(PolySignal::mono(Signal::Volts(amount))),
        gate: None,
    };
    let lanes = inversion_derive_lane_count(&params);
    let mut outputs = InversionOutputs::default();
    outputs.set_all_lanes(lanes);
    let mut inversion = Inversion {
        params,
        outputs,
        state: InversionState::default(),
        _lane_count: lanes,
        _channels: 1,
        _block_index: Default::default(),
        _connected: Default::default(),
    };
    inversion.init(SR);
    inversion
}

const SR: f32 = 48000.0;

/// Samples a level takes to fade fully in or out, with one to spare for
/// rounding.
fn fade_samples() -> usize {
    (1.0 / FadeIn::increment(DEFAULT_FADE_IN_SECS, SR)).ceil() as usize + 1
}

fn run(inversion: &mut Inversion, samples: usize) {
    for _ in 0..samples {
        inversion.update(SR);
    }
}

/// Each lane's pitch and level (0–1).
fn read(inversion: &Inversion) -> Vec<(f32, f32)> {
    (0..inversion.lane_count())
        .map(|lane| {
            (
                inversion.outputs.output.get(lane),
                inversion.outputs.level.get(lane) / 5.0,
            )
        })
        .collect()
}

/// The voices of `notes` at `amount`.
fn voices(notes: &[f32], amount: f32) -> Vec<(f32, f32)> {
    let mut inversion = make_inversion(notes, amount);
    inversion.update(48000.0);
    read(&inversion)
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

fn assert_close(actual: &[f32], expected: &[f32], context: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{context}: {actual:?} vs {expected:?}"
    );
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a - e).abs() < 1e-5,
            "{context}: {actual:?} vs {expected:?}"
        );
    }
}

/// A whole amount `k` (0 passes them through) sounds the notes with the
/// lowest `k` raised an octave, on one more lane than the input has notes.
#[test]
fn whole_amounts_raise_the_lowest_notes() {
    for notes in chords() {
        for k in 0..notes.len() {
            let v = voices(&notes, k as f32);
            assert_eq!(v.len(), notes.len() + 1);
            let mut expected: Vec<f32> = notes
                .iter()
                .enumerate()
                .map(|(i, &n)| if i < k { n + 1.0 } else { n })
                .collect();
            expected.sort_by(f32::total_cmp);
            assert_close(&sounding(&v), &expected, &format!("{notes:?} inv {k}"));
        }
    }
}

/// A negative amount drops the highest notes an octave.
#[test]
fn negative_amounts_drop_the_highest_notes() {
    let notes = maj7();
    let mut expected = vec![notes[3] - 1.0, notes[0], notes[1], notes[2]];
    expected.sort_by(f32::total_cmp);
    assert_eq!(sounding(&voices(&notes, -1.0)), expected);
}

/// Between whole amounts the moving note crossfades between its two
/// octaves, and the levels always sum to the number of notes.
#[test]
fn fractions_crossfade_the_moving_note() {
    let notes = maj7();
    let v = voices(&notes, 1.25);
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
    for notes in chords() {
        let n = notes.len() as f32;
        let span = n * (n + 1.0);
        let step = 0.01;
        let mut previous = voices(&notes, -span);
        let mut x = -span;
        while x < span {
            x += step;
            let current = voices(&notes, x);
            for (lane, ((p0, l0), (p1, l1))) in previous.iter().zip(&current).enumerate() {
                if p0 != p1 {
                    assert!(
                        *l0 <= 2.0 * step && *l1 <= 2.0 * step,
                        "{notes:?} at {x}: lane {lane} moved {p0} → {p1} at levels {l0}, {l1}"
                    );
                }
            }
            previous = current;
        }
    }
}

/// A NaN or infinite amount holds the voices, pitches and levels, where
/// they are.
#[test]
fn non_finite_amount_holds() {
    let mut inversion = make_inversion(&maj7(), 1.5);
    inversion.update(48000.0);
    let before = read(&inversion);
    for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        inversion.params.amount = LaneSum::from_poly(PolySignal::mono(Signal::Volts(v)));
        inversion.update(48000.0);
        assert_eq!(read(&inversion), before, "{v}");
    }
}

/// Changing the notes while the amount stays put plays the new notes at
/// that inversion once the levels have faded: the state a patch update
/// carries over holds no notes.
#[test]
fn new_notes_sound_after_a_patch_update() {
    let triad = semis(&[-12.0, -8.0, -5.0]);
    let minor7 = semis(&[-7.0, -4.0, 0.0, 3.0]);
    for (from, to) in [
        (maj7(), minor7.clone()),
        (triad.clone(), maj7()),
        (maj7(), triad),
    ] {
        let mut old = make_inversion(&from, 1.0);
        old.update(48000.0);
        let mut new = make_inversion(&to, 1.0);
        std::mem::swap(&mut new.state, &mut old.state);
        run(&mut new, fade_samples());
        let mut fresh = make_inversion(&to, 1.0);
        fresh.update(48000.0);
        assert_eq!(read(&new), read(&fresh), "{from:?} → {to:?}");
    }
}

/// An amount equal to the number of notes is the chord an octave up; its
/// negative, an octave down.
#[test]
fn whole_rotations_transpose_by_octaves() {
    for notes in chords() {
        let n = notes.len() as f32;
        for (amount, octaves) in [(n, 1.0), (-n, -1.0), (2.0 * n, 2.0)] {
            let expected: Vec<f32> = notes.iter().map(|v| v + octaves).collect();
            assert_close(
                &sounding(&voices(&notes, amount)),
                &expected,
                &format!("{notes:?} at {amount}"),
            );
        }
    }
}

/// Past ±notes × (notes + 1) the amount holds the voicing at its end.
#[test]
fn amount_holds_past_its_span() {
    let span = 4.0 * 5.0;
    assert_eq!(voices(&maj7(), span), voices(&maj7(), span + 7.3));
    assert_eq!(voices(&maj7(), -span), voices(&maj7(), -1e30));
}

/// The output has one lane more than the input, up to the port's limit.
#[test]
fn output_adds_a_lane() {
    assert_eq!(make_inversion(&maj7(), 0.0).lane_count(), 5);
}

/// An input that fills every lane inverts without its top lane.
#[test]
fn full_input_drops_its_top_lane() {
    let full: Vec<f32> = (0..PORT_MAX_LANES).map(|i| i as f32 / 12.0).collect();
    let v = voices(&full, 1.0);
    assert_eq!(v.len(), PORT_MAX_LANES);
    let mut expected = full[1..PORT_MAX_LANES - 1].to_vec();
    expected.push(full[0] + 1.0);
    expected.sort_by(f32::total_cmp);
    assert_close(&sounding(&v), &expected, "full");
}

/// A moving input is read every sample.
#[test]
fn follows_a_moving_input() {
    let mut inversion = make_inversion(&maj7(), 1.0);
    inversion.update(48000.0);
    inversion.params.input = pitches(&semis(&[-10.0, -6.0, -3.0, 1.0]));
    inversion.update(48000.0);
    let expected = semis(&[-6.0, -3.0, 1.0, 2.0]);
    assert_close(&sounding(&read(&inversion)), &expected, "moved");
}

/// Only the notes whose gate is high are inverted, as a chord of their
/// own; the lanes left over fall silent.
#[test]
fn gate_masks_input_notes() {
    let notes = maj7();
    let mut inversion = make_inversion(&notes, 1.0);
    inversion.params.gate = Some(pitches(&[5.0, 0.0, 5.0, 5.0]));
    inversion.update(48000.0);
    let v = read(&inversion);
    assert_eq!(v.len(), 5);
    let mut expected = vec![notes[2], notes[3], notes[0] + 1.0];
    expected.sort_by(f32::total_cmp);
    assert_close(&sounding(&v), &expected, "gated");
    let total: f32 = v.iter().map(|(_, level)| level).sum();
    assert!((total - 3.0).abs() < 1e-6, "{v:?}");
}

/// A narrower gate cycles across the input lanes.
#[test]
fn narrower_gate_cycles_across_input_lanes() {
    let notes = maj7();
    let mut inversion = make_inversion(&notes, 0.0);
    inversion.params.gate = Some(pitches(&[5.0, 0.0]));
    inversion.update(48000.0);
    assert_close(
        &sounding(&read(&inversion)),
        &[notes[0], notes[2]],
        "cycled",
    );
}

/// With every gate low the voices hold, pitches and levels, while the
/// notes and amount move.
#[test]
fn all_gates_low_holds() {
    let mut inversion = make_inversion(&maj7(), 1.5);
    inversion.params.gate = Some(pitches(&[5.0]));
    inversion.update(48000.0);
    let before = read(&inversion);
    inversion.params.gate = Some(pitches(&[0.0]));
    inversion.params.input = pitches(&semis(&[-7.0, -4.0, 0.0, 3.0]));
    inversion.params.amount = LaneSum::from_poly(PolySignal::mono(Signal::Volts(3.0)));
    inversion.update(48000.0);
    assert_eq!(read(&inversion), before);
}

/// Before any gate goes high the voices are silent.
#[test]
fn silent_until_a_gate_goes_high() {
    let mut inversion = make_inversion(&maj7(), 1.0);
    inversion.params.gate = Some(pitches(&[0.0]));
    inversion.update(48000.0);
    assert!(read(&inversion).iter().all(|&(_, level)| level == 0.0));
}

/// Notes arriving in any lane order invert as if listed lowest first.
#[test]
fn unsorted_notes_invert_as_sorted() {
    let notes = maj7();
    let shuffled = [notes[2], notes[0], notes[3], notes[1]];
    for amount in [0.0, 1.0, 2.5, -1.25] {
        assert_eq!(
            voices(&shuffled, amount),
            voices(&notes, amount),
            "{amount}"
        );
    }
    let mut gated = make_inversion(&shuffled, 1.0);
    gated.params.gate = Some(pitches(&[5.0, 5.0, 0.0, 5.0]));
    gated.update(48000.0);
    let mut sorted = make_inversion(&[notes[0], notes[1], notes[2]], 1.0);
    sorted.update(48000.0);
    assert_close(&sounding(&read(&gated)), &sounding(&read(&sorted)), "gated");
}

/// Each level moves by at most one fade step from `before` to `after`.
fn assert_levels_glide(before: &[(f32, f32)], after: &[(f32, f32)], context: &str) {
    let step = FadeIn::increment(DEFAULT_FADE_IN_SECS, SR);
    for (lane, ((_, l0), (_, l1))) in before.iter().zip(after).enumerate() {
        assert!(
            (l1 - l0).abs() <= step + 1e-6,
            "{context}: lane {lane} jumped {l0} → {l1}"
        );
    }
}

/// Switching a gate fades the voices in and out instead of stepping them,
/// and settles on the voicing of the notes left on.
#[test]
fn gate_switches_fade_levels() {
    let notes = maj7();
    for (from, to) in [
        ([5.0, 5.0, 5.0, 5.0], [5.0, 0.0, 5.0, 5.0]),
        ([5.0, 0.0, 5.0, 5.0], [5.0; 4]),
    ] {
        let mut inversion = make_inversion(&notes, 1.0);
        inversion.params.gate = Some(pitches(&from));
        inversion.update(SR);
        inversion.params.gate = Some(pitches(&to));
        let mut previous = read(&inversion);
        for _ in 0..fade_samples() {
            inversion.update(SR);
            let current = read(&inversion);
            assert_levels_glide(&previous, &current, &format!("{from:?} → {to:?}"));
            previous = current;
        }
        let mut settled = make_inversion(&notes, 1.0);
        settled.params.gate = Some(pitches(&to));
        settled.update(SR);
        assert_eq!(read(&inversion), read(&settled), "{from:?} → {to:?}");
    }
}

/// A voice fading out keeps the note it was playing until it is silent.
#[test]
fn a_fading_voice_keeps_its_note() {
    let notes = maj7();
    let mut inversion = make_inversion(&notes, 0.0);
    inversion.update(SR);
    let before = read(&inversion);
    inversion.params.gate = Some(pitches(&[0.0, 5.0, 5.0, 5.0]));
    for _ in 0..fade_samples() {
        inversion.update(SR);
        for (lane, ((p0, _), (p1, l1))) in before.iter().zip(read(&inversion)).enumerate() {
            if l1 > 0.0 && l1 < 1.0 && p1 != *p0 {
                panic!("lane {lane} changed note {p0} → {p1} at level {l1}");
            }
        }
    }
}

/// The first gate to go high fades the voices in from silence to the voicing
/// of the notes it turns on.
#[test]
fn first_gate_fades_in() {
    let mut inversion = make_inversion(&maj7(), 1.0);
    inversion.params.gate = Some(pitches(&[0.0]));
    inversion.update(SR);
    inversion.params.gate = Some(pitches(&[5.0]));
    let mut previous = read(&inversion);
    for _ in 0..fade_samples() {
        inversion.update(SR);
        let current = read(&inversion);
        assert_levels_glide(&previous, &current, "onset");
        previous = current;
    }
    let mut settled = make_inversion(&maj7(), 1.0);
    settled.update(SR);
    assert_eq!(read(&inversion), read(&settled));
}

/// Swaps `old`'s state into `new` as a patch update does.
fn patch_update(old: &mut Inversion, new: &mut Inversion) {
    std::mem::swap(&mut new.state, &mut old.state);
    crate::types::PatchUpdateHandler::on_patch_update(new);
}

/// A patch edit that changes the notes or the amount fades the levels to the
/// new voicing; a lane taking another note fades out and back in.
#[test]
fn patch_updates_fade_levels() {
    let mut old = make_inversion(&semis(&[-12.0, -8.0, -5.0]), 0.0);
    old.update(SR);
    let mut new = make_inversion(&maj7(), 1.5);
    let before = read(&old);
    patch_update(&mut old, &mut new);
    new.update(SR);
    let mut previous = read(&new);
    assert_levels_glide(&before, &previous, "edit");
    for _ in 0..2 * fade_samples() {
        new.update(SR);
        let current = read(&new);
        assert_levels_glide(&previous, &current, "edit");
        previous = current;
    }
    let mut settled = make_inversion(&maj7(), 1.5);
    settled.update(SR);
    assert_eq!(read(&new), read(&settled));
}

/// A lane an edit drops and a later edit brings back fades in from silence.
#[test]
fn returning_lanes_fade_in() {
    let mut wide = make_inversion(&maj7(), 0.0);
    wide.update(SR);
    let mut narrow = make_inversion(&maj7()[..2], 0.0);
    patch_update(&mut wide, &mut narrow);
    run(&mut narrow, fade_samples());
    let mut back = make_inversion(&maj7(), 0.0);
    patch_update(&mut narrow, &mut back);
    back.update(SR);
    let silent = vec![(0.0, 0.0); back.lane_count()];
    assert_levels_glide(&silent[2..], &read(&back)[2..], "returning");
}

/// However fast the amount sweeps, a voice takes a new note only once it
/// has faded to silence (to within one fade step).
#[test]
fn fast_sweeps_change_notes_only_while_silent() {
    let notes = maj7();
    let step = FadeIn::increment(DEFAULT_FADE_IN_SECS, SR);
    for hz in [20.0_f32, 40.0, 200.0] {
        let mut inversion = make_inversion(&notes, 2.0);
        inversion.update(SR);
        let mut previous = read(&inversion);
        for i in 1..(SR as usize / 4) {
            let t = i as f32 / SR;
            let amount = 2.0 + 2.0 * (std::f32::consts::TAU * hz * t).sin();
            inversion.params.amount = LaneSum::from_poly(PolySignal::mono(Signal::Volts(amount)));
            inversion.update(SR);
            let current = read(&inversion);
            for (lane, ((p0, l0), (p1, _))) in previous.iter().zip(&current).enumerate() {
                assert!(
                    p0 == p1 || *l0 <= step,
                    "{hz} Hz at {t}: lane {lane} moved {p0} → {p1} at level {l0}"
                );
            }
            assert_levels_glide(&previous, &current, &format!("{hz} Hz"));
            previous = current;
        }
    }
}
