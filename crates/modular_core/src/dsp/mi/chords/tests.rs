use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::utils::voct_to_hz;

/// `seconds` of the chord and its root at `hz` (negative runs backward),
/// knobs held.
fn render(sample_rate: f32, hz: f32, knobs: [f32; 3], seconds: f32) -> Vec<(f32, f32)> {
    let state = MiChordsState::new(sample_rate);
    let mut lane = Lane::default();
    (0..(sample_rate * seconds) as usize)
        .map(|_| lane.step(hz / sample_rate, knobs, &state))
        .collect()
}

fn chord(sample_rate: f32, hz: f32, knobs: [f32; 3], seconds: f32) -> Vec<f32> {
    render(sample_rate, hz, knobs, seconds)
        .into_iter()
        .map(|(chord, _)| chord)
        .collect()
}

/// Replays `chords.f32` (Plaits' chord engine rebuilt from its unmodified
/// parts, a sample at a time): chords swept both ways, inversions and the
/// waveform swept, pitch steps and sweeps on the organ and the wavetable, a
/// pitch high enough to shift octaves and fade, and waveform jumps.
#[test]
fn matches_plaits() {
    let state = MiChordsState::new(48000.0);
    let mut lane = Lane::default();
    let mut actual = (Vec::new(), Vec::new());
    let mut expected = (Vec::new(), Vec::new());
    for record in records("chords", 4 + 2) {
        let (chord, root) = lane.step(record[0], [record[1], record[2], record[3]], &state);
        actual.0.push(chord);
        actual.1.push(root);
        expected.0.push(record[4]);
        expected.1.push(record[5]);
    }
    for (name, actual, expected) in [
        ("chord", &actual.0, &expected.0),
        ("root", &actual.1, &expected.1),
    ] {
        // The port scales the wavetable's differences before its filters and
        // Plaits after them, which rounds differently where a voice resumes
        // on a large jump in the wave.
        let (error, at) = max_error(actual, expected);
        assert!(error < 1e-4, "{name}: max error {error} at sample {at}");
    }
}

/// The pitch the octave chord with no inversion sounds at `hz`: its root two
/// octaves down, slightly flat, and a touch sharper on the wavetable, with
/// its other notes on that root's harmonics.
fn octave_chord_hz(hz: f32, waveform: f32) -> f32 {
    let wavetable = if knob(waveform) > 0.6 { 1.004 } else { 1.0 };
    hz * 0.25 * 0.998 * wavetable
}

/// A steady chord has the same harmonics at 44.1 and 96 kHz as at 48 kHz, on
/// the organ and on the wavetable.
#[test]
fn sounds_the_same_at_any_rate() {
    for (pitch, waveform) in [(1.0, 0.0), (0.0, 1.5), (1.0, 4.0), (2.0, 5.0)] {
        let hz = voct_to_hz(pitch);
        assert_rate_independent(
            |sample_rate| chord(sample_rate, hz, [0.0, 0.0, waveform], 0.5),
            octave_chord_hz(hz, waveform),
            0.02,
        );
    }
}

/// Asserts `backward` has `forward`'s first 12 harmonics of `hz`, within 2%
/// of the loudest.
fn assert_same_harmonics(name: &str, forward: &[f32], backward: &[f32], hz: f32) {
    let forward = harmonics(forward, hz, 48000.0, 12);
    let backward = harmonics(backward, hz, 48000.0, 12);
    let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
    for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
        assert!(
            (b - f).abs() < 0.02 * loudest,
            "{name}, harmonic {}: {b} vs {f}",
            h + 1
        );
    }
}

/// Through-zero FM's backward pitch plays the same harmonics as forward on
/// the organ. A wavetable chord's notes share harmonics whose phases its
/// smoothing shifts by each note's own pitch, so their sum differs running
/// backward; each wavetable voice on its own sounds the same.
#[test]
fn runs_backward() {
    let hz = 800.0;
    for waveform in [0.0, 1.5] {
        let knobs = [0.0, 0.0, waveform];
        assert_same_harmonics(
            &format!("organ {waveform}"),
            &chord(48000.0, hz, knobs, 0.5),
            &chord(48000.0, -hz, knobs, 0.5),
            octave_chord_hz(hz, waveform),
        );
    }
    let f0 = 200.0 / 48000.0;
    for scan in [0.0, 0.37, 0.8, 1.0] {
        let tone = |f0: f32| -> Vec<f32> {
            let mut voice = wavetable::WavetableVoice::default();
            (0..24000).map(|_| voice.step(f0, f0, 0.25, scan)).collect()
        };
        assert_same_harmonics(&format!("wave {scan}"), &tone(f0), &tone(-f0), 200.0);
    }
}

/// The root output is the chord's root voices alone, three times as loud.
#[test]
fn root_is_the_chord_root() {
    for inversion in [0.0, 1.2, 2.5, 4.9] {
        let knobs = [10.0, inversion, 1.0];
        let hz = voct_to_hz(0.0);
        let rendered = render(48000.0, hz, knobs, 0.5);
        let root: Vec<f32> = rendered.iter().map(|&(_, root)| root).collect();
        let lane = {
            let mut lane = Lane::default();
            lane.control(knobs, &MiChordsState::new(48000.0));
            lane
        };
        let lowest = (0..NUM_VOICES)
            .filter(|v| lane.inversion.root_mask & (1 << v) != 0)
            .map(|v| lane.inversion.ratios[v])
            .fold(f32::INFINITY, f32::min);
        // The root's own pitch carries its level, not the chord's other notes.
        let at_root = harmonics(&root, hz * 0.998 * lowest, 48000.0, 1)[0];
        let at_third = harmonics(&root, hz * 0.998 * lowest * 1.259_921, 48000.0, 1)[0];
        assert!(
            at_root > 20.0 * at_third,
            "inversion {inversion}: root {at_root} vs third {at_third}"
        );
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = MiChordsState::new(48000.0);
    let mut held = Lane::default();
    let mut glitched = Lane::default();
    for n in 0..600 {
        let v = match n % 200 {
            100 => f32::NAN,
            101 => f32::INFINITY,
            102 => f32::NEG_INFINITY,
            _ => 2.7,
        };
        for k in 0..3 {
            let mut knobs = [2.7; 3];
            let a = held.step(0.01, knobs, &state);
            knobs[k] = v;
            let b = glitched.step(0.01, knobs, &state);
            assert_eq!(a, b, "knob {k}, sample {n}");
        }
    }
}

/// Every chord, voicing and waveform at any pitch, including NaN, infinite
/// and backward ones, at any rate, leave both outputs finite and within full
/// scale.
#[test]
fn stays_finite_and_bounded() {
    for sample_rate in [8000.0, 44100.0, 96000.0] {
        let state = MiChordsState::new(sample_rate);
        let mut lane = Lane::default();
        for n in 0..60000 {
            let f0 = match n % 700 {
                100 => f32::NAN,
                150 => f32::INFINITY,
                200 => f32::NEG_INFINITY,
                _ => (n % 13) as f32 * 0.02 - 0.06,
            };
            let t = n as f32;
            let knobs = [
                5.0 + 5.5 * (t * 0.0007).sin(),
                2.5 + 2.6 * (t * 0.0011).sin(),
                2.5 + 2.6 * (t * 0.0005).sin(),
            ];
            let (chord, root) = lane.step(f0, knobs, &state);
            for out in [chord, root] {
                assert!(
                    out.is_finite() && (OUTPUT_GAIN * out).abs() <= 5.5,
                    "{sample_rate} Hz, sample {n}: {out}"
                );
            }
        }
    }
}
