use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(brightness: f32, envelope: f32, accent: f32) -> Knobs {
    Knobs {
        brightness: (brightness, mark()),
        envelope: (envelope, mark()),
        accent: (accent, mark()),
    }
}

/// Plaits' 96 patches, bank by bank, from the builtin files.
fn voices() -> &'static [Dx7Voice] {
    static VOICES: std::sync::LazyLock<Vec<Dx7Voice>> = std::sync::LazyLock::new(|| {
        test_voices::PLAITS_VOICES
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let v = parse_syx(&test_voices::plaits_file(index))
                    .unwrap()
                    .remove(0);
                Dx7Voice::new(path.to_string(), v.name, v.packed)
            })
            .collect()
    });
    &VOICES
}

/// Plaits' patch `patch` of `bank`.
fn voice(bank: usize, patch: usize) -> &'static Dx7Voice {
    &voices()[bank * 32 + patch]
}

/// The builtin voice of file stem `name`.
fn builtin(name: &str) -> &'static Dx7Voice {
    let file = format!("/{name}.syx");
    let index = test_voices::PLAITS_VOICES
        .iter()
        .position(|p| p.ends_with(&file))
        .unwrap();
    &voices()[index]
}

fn new_lane(seed: u32) -> Lane {
    Lane {
        random: Random::new(seed),
        ..Lane::default()
    }
}

/// Replays `six_op.f32` (Plaits' six-op engine with per-sample controls):
/// gated notes across all three banks, feedback loops spanning several
/// operators, fixed-frequency operators, pitch envelopes and LFOs, notes
/// alternating between patches, scrubbed drones, and one to three voices.
#[test]
fn matches_plaits() {
    let state = SixOpState::new(48000.0);
    let mut current = -1.0;
    let mut lane = new_lane(0);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("six_op", 10) {
        if record[0] != current {
            current = record[0];
            lane = new_lane(0x5eed + current as u32);
        }
        let gate = match record[5] {
            g if g < 0.0 => None,
            g => Some(g > 0.0),
        };
        let out = lane.step(
            record[1],
            knobs(record[2], record[3], record[4]),
            gate,
            voice(record[6] as usize, record[7] as usize),
            record[8] as usize,
            1.0,
            &state,
        );
        actual.push(out);
        expected.push(record[9]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {at}");
}

/// Plaits has a renderer for every chain of every algorithm.
#[test]
fn every_algorithm_compiles() {
    for algorithm in 0..32 {
        algorithms::Algorithm::compile(algorithm);
    }
}

/// `seconds` of a note at `hz` on `bank`/`patch`, at `sample_rate`: held for
/// the first half when `gated`, otherwise scrubbed at `envelope`.
fn render(
    sample_rate: f32,
    hz: f32,
    bank: usize,
    patch: usize,
    gated: bool,
    envelope: f32,
    seconds: f32,
) -> Vec<f32> {
    let state = SixOpState::new(sample_rate);
    let mut lane = new_lane(1);
    let samples = (sample_rate * seconds) as usize;
    (0..samples)
        .map(|n| {
            let gate = gated.then_some(n < samples / 2);
            lane.step(
                hz / sample_rate,
                knobs(2.5, envelope, 4.0),
                gate,
                voice(bank, patch),
                2,
                1.0,
                &state,
            )
        })
        .collect()
}

/// A scrubbed drone has the same harmonics at 44.1 and 96 kHz as at 48 kHz.
/// Operator feedback reads the last two samples, so the strongest feedback
/// sounds a little different at other rates.
#[test]
fn sounds_the_same_at_any_rate() {
    for (bank, patch, envelope, tolerance) in [
        (0, 2, 2.5, 0.02),
        (2, 1, 2.5, 0.02),
        (1, 0, 2.5, 0.02),
        (0, 13, 3.0, 0.02),
        (2, 29, 2.0, 0.05),
    ] {
        let hz = voct_to_hz(-1.0);
        assert_rate_independent(
            |sample_rate| render(sample_rate, hz, bank, patch, false, envelope, 0.5),
            hz,
            tolerance,
        );
    }
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// A played note's envelopes take the same time at any rate: its level in
/// 20 ms windows through attack, decay and release.
#[test]
fn envelopes_take_the_same_time_at_any_rate() {
    let windows = |sample_rate: f32, bank: usize, patch: usize, envelope: f32| {
        let out = render(sample_rate, 220.0, bank, patch, true, envelope, 1.0);
        let window = (0.02 * sample_rate) as usize;
        out.chunks_exact(window).map(rms).collect::<Vec<_>>()
    };
    for (bank, patch, envelope) in [(1, 0, 2.5), (0, 0, 1.0), (1, 20, 4.0)] {
        let reference = windows(48000.0, bank, patch, envelope);
        let loudest = reference.iter().fold(0.0f32, |m, &a| m.max(a));
        for sample_rate in [44100.0, 96000.0] {
            let actual = windows(sample_rate, bank, patch, envelope);
            for (w, (a, r)) in actual.iter().zip(&reference).enumerate() {
                if *r > 0.01 * loudest {
                    let db = 20.0 * (a / r).log10();
                    assert!(
                        db.abs() < 1.0,
                        "bank {bank} patch {patch}, {sample_rate} Hz, window {w}: {db} dB"
                    );
                }
            }
        }
    }
}

/// Through-zero FM runs the operators backward: without feedback, a negative
/// frequency plays the forward waveform inverted.
#[test]
fn runs_backward() {
    let state = SixOpState::new(48000.0);
    // Patches without feedback or fixed-frequency operators.
    for (bank, patch) in [(0, 2), (2, 1)] {
        let render = |f0: f32| {
            let mut lane = new_lane(1);
            (0..4800)
                .map(|_| {
                    lane.step(
                        f0,
                        knobs(2.5, 2.5, 4.0),
                        None,
                        voice(bank, patch),
                        2,
                        1.0,
                        &state,
                    )
                })
                .collect::<Vec<_>>()
        };
        let forward = render(0.004);
        let backward = render(-0.004);
        let peak = forward.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let error = forward
            .iter()
            .zip(&backward)
            .fold(0.0f32, |m, (a, b)| m.max((a + b).abs()));
        assert!(peak > 0.1, "bank {bank} patch {patch}: silent");
        assert!(
            error < 1e-3 * peak,
            "bank {bank} patch {patch}: mirror error {error}"
        );
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = SixOpState::new(48000.0);
    let mut held = new_lane(7);
    let mut glitched = new_lane(7);
    for n in 0..4000 {
        let v = match n {
            500 => f32::NAN,
            501 => f32::INFINITY,
            502 => f32::NEG_INFINITY,
            _ => 2.0,
        };
        let gate = Some(n % 1000 < 700);
        let a = held.step(
            0.005,
            knobs(2.0, 2.0, 2.0),
            gate,
            voice(2, 2),
            2,
            1.0,
            &state,
        );
        let b = glitched.step(0.005, knobs(v, v, v), gate, voice(2, 2), 2, 1.0, &state);
        assert_eq!(a, b, "sample {n}");
    }
}

/// Every patch stays finite and within full scale at extreme pitches, knob
/// jumps and any engine rate, including a NaN or infinite pitch.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 22050.0, 192000.0] {
        let state = SixOpState::new(sample_rate);
        for bank in 0..3 {
            for patch in 0..32 {
                let mut lane = new_lane(3);
                for n in 0..3000 {
                    let f0 = match n % 7 {
                        0 => 0.5,
                        1 => -0.5,
                        2 if n % 500 == 2 => f32::NAN,
                        3 if n % 500 == 3 => f32::INFINITY,
                        _ => 0.0005 + (n as f32 * 0.001).sin() * 0.05,
                    };
                    let knob = (n % 11) as f32 * 0.5;
                    let gate = (bank != 2).then_some(n % 400 < 250);
                    let out = lane.step(
                        f0,
                        knobs(knob, 5.0 - knob, 5.0),
                        gate,
                        voice(bank, patch),
                        2,
                        1.0,
                        &state,
                    );
                    assert!(
                        out.is_finite() && out.abs() <= ACCENT_TRIM,
                        "{sample_rate} Hz, bank {bank} patch {patch}, sample {n}: {out}"
                    );
                }
            }
        }
    }
}

/// Levels in 10 ms windows.
fn envelope_of(x: &[f32], sample_rate: f32) -> Vec<f32> {
    x.chunks((sample_rate * 0.01) as usize).map(rms).collect()
}

/// `envelope` runs from snappy (0) to slow (5), and the release is shortest
/// around 1.5.
#[test]
fn envelope_knob_runs_from_snappy_to_slow() {
    let sample_rate = 48000.0;
    // Windows until the attack reaches half its peak.
    let attack = |envelope: f32| {
        let levels = envelope_of(
            &render(sample_rate, 261.63, 2, 20, true, envelope, 4.0),
            sample_rate,
        );
        let held = &levels[..levels.len() / 2];
        let peak = held.iter().fold(0.0f32, |m, &v| m.max(v));
        held.iter().position(|&v| v >= 0.5 * peak).unwrap()
    };
    let (fast, middle, slow) = (attack(0.0), attack(2.5), attack(5.0));
    assert!(
        fast < middle && middle < slow,
        "attack windows: {fast}, {middle}, {slow}"
    );

    // Windows from the gate's fall until the level is below a tenth of its
    // level at the fall.
    let release = |envelope: f32| {
        let levels = envelope_of(
            &render(sample_rate, 261.63, 2, 11, true, envelope, 8.0),
            sample_rate,
        );
        let fall = levels.len() / 2;
        let at_fall = levels[fall - 1];
        levels[fall..]
            .iter()
            .position(|&v| v < 0.1 * at_fall)
            .unwrap_or(levels.len())
    };
    let (zero, short, five) = (release(0.0), release(1.5), release(5.0));
    assert!(
        short < zero && short < five,
        "release windows: {zero}, {short}, {five}"
    );
}

/// Unpatching the gate forgets its last state, so a gate patched again while
/// high still rises: the note it starts loads the voice set then.
#[test]
fn a_gate_patched_again_rises() {
    let state = SixOpState::new(48000.0);
    let mut lane = new_lane(1);
    for _ in 0..100 {
        lane.step(
            0.005,
            knobs(2.5, 2.5, 4.0),
            Some(true),
            voice(0, 0),
            2,
            1.0,
            &state,
        );
    }
    assert!(lane.gate);
    lane.step(
        0.005,
        knobs(2.5, 2.5, 4.0),
        None,
        voice(0, 0),
        2,
        1.0,
        &state,
    );
    assert!(!lane.gate);
}

/// A note-on while the last note still sounds starts the new note in a
/// silent voice, so a voice whose operators restart their phases at note-on
/// leaves the sounding note to release: on the note-on's first sample the
/// output is the release alone, give or take the new note's first step.
#[test]
fn a_retrigger_leaves_the_last_note_sounding() {
    let state = SixOpState::new(48000.0);
    let step = |lane: &mut Lane, gate: bool, voice: &Dx7Voice| {
        lane.step(
            220.0 / 48000.0,
            knobs(2.5, 2.5, 0.0),
            Some(gate),
            voice,
            2,
            1.0,
            &state,
        )
    };
    let mut worst = (0.0f32, "");
    for (index, path) in test_voices::PLAITS_VOICES.iter().enumerate() {
        let voice = &voices()[index];
        if !voice.patch().reset_phase {
            continue;
        }
        let mut lane = new_lane(1);
        let peak = (0..4800)
            .map(|n| step(&mut lane, n < 4752, voice).abs())
            .fold(0.0f32, f32::max);
        let mut released = lane;
        let error = (step(&mut lane, true, voice) - step(&mut released, false, voice)).abs();
        if error / peak > worst.0 {
            worst = (error / peak, path);
        }
    }
    assert!(
        worst.0 < 0.05,
        "{}: {} of the note's peak",
        worst.1,
        worst.0
    );
}

/// The sharpest step (largest second difference) in the 60 samples after
/// each note-on of a minor phrase from the second note on: a note every
/// quarter second, held for half of it.
fn note_on_steps(voice: &Dx7Voice, (b, e, a): (f32, f32, f32), voices: usize) -> [f32; 7] {
    let state = SixOpState::new(48000.0);
    let semitones = [0.0f32, 3.0, 2.0, 7.0];
    let mut lane = new_lane(1);
    let out: Vec<f32> = (0..96000)
        .map(|n| {
            let f0 = voct_to_hz(-2.0 + semitones[n / 12000 % 4] / 12.0) / 48000.0;
            let gate = Some(n % 12000 < 6000);
            lane.step(f0, knobs(b, e, a), gate, voice, voices, 1.0, &state)
        })
        .collect();
    std::array::from_fn(|i| {
        let n = (i + 1) * 12000;
        (n..n + 60)
            .map(|m| (out[m] - 2.0 * out[m - 1] + out[m - 2]).abs())
            .fold(0.0f32, f32::max)
    })
}

/// A fast attack rises over about as long as Plaits' block, not in a sample:
/// bells with a snappy envelope, notes starting in the last ones' tails, step
/// no more at a note-on than Plaits' own engine does (0.0114 there), whether
/// the voice playing it is fresh or one a newer note faded out.
#[test]
fn a_fast_attack_ramps() {
    for voices in [1, 2, 3, 8] {
        let steps = note_on_steps(builtin("bells"), (0.0, 0.17, 0.0), voices);
        for (note, step) in steps.iter().enumerate() {
            assert!(
                *step < 0.012,
                "{voices} voices, note {note}: second difference {step}"
            );
        }
    }
}

/// A note beyond `voices` fades the oldest out within a few milliseconds,
/// leaving the newer notes sounding as if it had never played.
#[test]
fn a_note_past_the_count_fades_the_oldest() {
    let state = SixOpState::new(48000.0);
    let e_piano = builtin("e-piano-1");
    let render = |first: bool| {
        let mut lane = new_lane(1);
        (0..24000)
            .map(|n| {
                let gate = if n < 12000 {
                    first && n < 6000
                } else {
                    n < 18000
                };
                lane.step(
                    0.004,
                    knobs(2.5, 2.5, 0.0),
                    Some(gate),
                    e_piano,
                    1,
                    1.0,
                    &state,
                )
            })
            .collect::<Vec<f32>>()
    };
    let (both, second) = (render(true), render(false));
    let faded = 12000 + (0.006 * 48000.0) as usize;
    assert!(both[12000..faded] != second[12000..faded]);
    assert_eq!(both[faded..], second[faded..]);
}

/// Fading out the oldest note while a new one starts never steps the output,
/// even for cs-80, which restarts its phases at every note-on and has three
/// operators that never fall silent.
#[test]
fn fading_the_oldest_note_never_steps() {
    let cs80 = builtin("cs-80");
    assert!(cs80.patch().reset_phase);
    for (note, step) in note_on_steps(cs80, (2.23, 2.17, 1.075), 2)
        .iter()
        .enumerate()
    {
        assert!(*step < 0.02, "note {note}: second difference {step}");
    }
}

/// Each note plays at the level its own accent set: a new note leaves the
/// notes still sounding at theirs.
#[test]
fn each_note_keeps_its_accent() {
    let state = SixOpState::new(48000.0);
    // A voice without LFO depth, so the notes' modulations agree whichever
    // note is the active one.
    let voice = voices()
        .iter()
        .find(|v| v.patch().lfo.amp_mod_depth == 0.0 && v.patch().lfo.pitch_mod_depth == 0.0)
        .unwrap();
    let render = |second_note: bool| {
        let mut lane = new_lane(1);
        (0..6000)
            .map(|n| {
                let (gate, level) = if n < 2100 {
                    (n < 2000, 2.0)
                } else {
                    (second_note, 0.0)
                };
                lane.step(
                    0.004,
                    knobs(2.5, 2.5, 0.0),
                    Some(gate),
                    voice,
                    2,
                    level,
                    &state,
                )
            })
            .collect::<Vec<f32>>()
    };
    let (silent_second, first_alone) = (render(true), render(false));
    assert!(first_alone[2100..2500].iter().any(|v| v.abs() > 1e-3));
    assert_eq!(silent_second, first_alone);
}
