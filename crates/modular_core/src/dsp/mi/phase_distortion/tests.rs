use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::utils::voct_to_hz;
use crate::types::{OutputStruct, Signal};

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(ratio: f32, amount: f32, asymmetry: f32) -> Knobs {
    offset_knobs(ratio, amount, asymmetry, 0.0)
}

fn offset_knobs(ratio: f32, amount: f32, asymmetry: f32, offset: f32) -> Knobs {
    Knobs {
        ratio: (ratio, mark()),
        amount: (amount, mark()),
        asymmetry: (asymmetry, mark()),
        offset: (offset, mark()),
    }
}

fn render(voice: &mut Voice, f0: f32, k: Knobs, sync: bool, state: &PhaseDistortionState) -> f32 {
    let controls = voice.control(f0, k, state);
    voice.step(controls, sync, state)
}

/// Replays `phase_distortion.f32` (Plaits' phase distortion engine on its own
/// oscillator, with per-sample controls), synced and free-running.
#[test]
fn matches_plaits() {
    let state = PhaseDistortionState::new(48000.0);
    let mut synced = Voice::default();
    let mut free = Voice::default();
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for record in records("phase_distortion", 4 + 2) {
        let k = || knobs(record[1], record[2], record[3]);
        actual.push(render(&mut synced, record[0], k(), true, &state));
        actual.push(render(&mut free, record[0], k(), false, &state));
        expected.extend([record[4], record[5]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// A pitch running backward plays the forward waveform reversed in time, so
/// its harmonics match the forward ones. The triangle reaches the output one
/// substep after the phasor, as in Plaits, which reversal turns into a lead,
/// so the match is close at a gentle `amount` and loosens as it rises.
#[test]
fn runs_backward() {
    let sample_rate = 48000.0;
    let state = PhaseDistortionState::new(sample_rate);
    for sync in [true, false] {
        for (hz, ratio, asymmetry) in [
            (110.0, 2.5, 0.0),
            (220.0, 4.0, 3.0),
            (55.0, 1.0, 5.0),
            (440.0, 3.75, 1.5),
        ] {
            let tone = |f0: f32| {
                let mut voice = Voice::default();
                let out: Vec<f32> = (0..48000)
                    .map(|_| render(&mut voice, f0, knobs(ratio, 1.0, asymmetry), sync, &state))
                    .collect();
                assert!(out.iter().all(|v| v.is_finite()));
                harmonics(&out[4800..], hz, sample_rate, 16)
            };
            let forward = tone(hz / sample_rate);
            let backward = tone(-hz / sample_rate);
            let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
            for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
                assert!(
                    (b - f).abs() < 0.05 * loudest,
                    "sync {sync}, {hz} Hz, harmonic {}: {b} vs {f}",
                    h + 1
                );
            }
        }
    }
}

/// Checked on harmonic tones: synced, or free-running at a ×2 or ×4 ratio
/// (the table's plateaus at 2.383 and 3.75), and gentle enough not to alias
/// at 44.1 kHz.
#[test]
fn sounds_the_same_at_any_rate() {
    for (sync, pitch, ratio, amount, asymmetry) in [
        (true, -1.0, 2.5, 2.0, 0.0),
        (true, 0.0, 3.5, 2.0, 3.0),
        (false, -1.0, 2.383, 2.0, 0.0),
        (false, 0.0, 3.75, 2.0, 3.0),
    ] {
        assert_rate_independent(
            |sample_rate| {
                let state = PhaseDistortionState::new(sample_rate);
                let mut voice = Voice::default();
                let f0 = voct_to_hz(pitch) / sample_rate;
                (0..(sample_rate * 0.5) as usize)
                    .map(|_| {
                        render(
                            &mut voice,
                            f0,
                            knobs(ratio, amount, asymmetry),
                            sync,
                            &state,
                        )
                    })
                    .collect()
            },
            voct_to_hz(pitch),
            0.02,
        );
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let state = PhaseDistortionState::new(48000.0);
    for sync in [true, false] {
        let mut held = Voice::default();
        let mut glitched = Voice::default();
        for n in 0..200 {
            let v = match n {
                100 => f32::NAN,
                101 => f32::INFINITY,
                102 => f32::NEG_INFINITY,
                _ => 1.0,
            };
            let a = render(&mut held, 0.01, knobs(1.0, 1.0, 1.0), sync, &state);
            let b = render(&mut glitched, 0.01, knobs(v, v, v), sync, &state);
            assert!((a - b).abs() < 1e-6, "sync {sync}, sample {n}: {a} vs {b}");
        }
    }
}

/// Through-zero sweeps, standstills, knob jumps, non-finite pitches and the
/// frequency limit at any engine rate leave the output finite and bounded.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 24000.0, 44100.0, 48000.0, 96000.0] {
        let state = PhaseDistortionState::new(sample_rate);
        for sync in [true, false] {
            let mut voice = Voice::default();
            for n in 0..6000 {
                let f0 = match n % 7 {
                    0 => 2.0 * state.max_frequency,
                    1 => -2.0 * state.max_frequency,
                    2 => 0.0,
                    3 if n % 500 == 3 => f32::NAN,
                    4 if n % 500 == 4 => f32::INFINITY,
                    _ => (n as f32 * 0.003).sin() * 0.02,
                };
                let knob = (n % 11) as f32 * 0.5;
                let offset = match n % 13 {
                    0 if n % 600 == 0 => f32::NAN,
                    1 if n % 600 == 1 => f32::NEG_INFINITY,
                    5 => -7.25,
                    _ => (n as f32 * 0.007).sin() * 3.0,
                };
                let k = offset_knobs(knob, 5.0 - knob, knob, offset);
                let out = render(&mut voice, f0, k, sync, &state);
                assert!(out.is_finite(), "{sample_rate} Hz, sync {sync}, sample {n}");
            }
        }
    }
}

/// Holding the frequency limit, which passes a whole cycle per substep below
/// 12 kHz, leaves no phase behind: the tone after it matches a fresh one.
#[test]
fn recovers_from_the_frequency_limit() {
    for sample_rate in [8000.0, 11025.0, 48000.0] {
        let state = PhaseDistortionState::new(sample_rate);
        for sync in [true, false] {
            for limit in [2.0 * state.max_frequency, -2.0 * state.max_frequency] {
                let hz = 220.0;
                let tone = |voice: &mut Voice| {
                    let out: Vec<f32> = (0..(sample_rate * 0.5) as usize)
                        .map(|_| {
                            render(voice, hz / sample_rate, knobs(2.5, 2.0, 1.0), sync, &state)
                        })
                        .collect();
                    harmonics(&out[(sample_rate * 0.1) as usize..], hz, sample_rate, 8)
                };
                let fresh = tone(&mut Voice::default());
                let mut voice = Voice::default();
                for _ in 0..sample_rate as usize {
                    render(&mut voice, limit, knobs(2.5, 2.0, 1.0), sync, &state);
                }
                let after = tone(&mut voice);
                let loudest = fresh.iter().fold(0.0f32, |m, &a| m.max(a));
                for (h, (a, f)) in after.iter().zip(&fresh).enumerate() {
                    assert!(
                        (a - f).abs() < 0.05 * loudest,
                        "{sample_rate} Hz, sync {sync}, limit {limit}, harmonic {}: {a} vs {f}",
                        h + 1
                    );
                }
            }
        }
    }
}

/// At a strong `amount` backward departs from the mirror only by the
/// triangle's one-substep lag, so the departure shrinks as the rate rises.
#[test]
fn backward_departs_only_by_the_substep_lag() {
    let departure = |sample_rate: f32| {
        let state = PhaseDistortionState::new(sample_rate);
        let hz = 220.0;
        let tone = |f0: f32| {
            let mut voice = Voice::default();
            let out: Vec<f32> = (0..sample_rate as usize)
                .map(|_| render(&mut voice, f0, knobs(4.0, 5.0, 3.0), true, &state))
                .collect();
            harmonics(&out[(sample_rate * 0.1) as usize..], hz, sample_rate, 16)
        };
        let forward = tone(hz / sample_rate);
        let backward = tone(-hz / sample_rate);
        let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
        forward
            .iter()
            .zip(&backward)
            .fold(0.0f32, |m, (f, b)| m.max((f - b).abs()))
            / loudest
    };
    let at_48k = departure(48000.0);
    let at_192k = departure(192000.0);
    assert!(
        at_192k < 0.4 * at_48k,
        "departure {at_192k} at 192 kHz vs {at_48k} at 48 kHz"
    );
}

/// A phase offset plays the same waveform shifted in time, synced or free
/// running: at a 100-sample period, a quarter-cycle offset leads by 25 samples.
#[test]
fn phase_offset_shifts_the_waveform_in_time() {
    let state = PhaseDistortionState::new(48000.0);
    for sync in [true, false] {
        for (ratio, amount, asymmetry) in [(2.5, 3.0, 0.0), (3.2, 4.0, 3.0), (0.7, 2.0, 5.0)] {
            let tone = |offset: f32| {
                let mut voice = Voice::default();
                (0..4000)
                    .map(|_| {
                        let k = offset_knobs(ratio, amount, asymmetry, offset);
                        render(&mut voice, 0.01, k, sync, &state)
                    })
                    .collect::<Vec<f32>>()
            };
            let ahead = tone(0.25);
            let plain = tone(0.0);
            let (error, at) = max_error(&ahead[1000..3000], &plain[1025..3025]);
            assert!(
                error < 1e-3,
                "sync {sync}, ratio {ratio}: error {error} at {at}"
            );
        }
    }
}

/// An offset that moves, or jumps, without crossing the wrap ends where a
/// voice started at it plays, within the rounding its phases gather on the
/// way.
#[test]
fn moving_phase_offset_lands_on_the_held_one() {
    let state = PhaseDistortionState::new(48000.0);
    for sync in [true, false] {
        let mut held = Voice::default();
        let mut glided = Voice::default();
        let mut jumped = Voice::default();
        for n in 0..6000 {
            let glide = (n as f32 / 2000.0).min(1.0) * 0.4;
            let jump = if n < 100 { 0.7 } else { 0.4 };
            let a = render(
                &mut held,
                0.01,
                offset_knobs(3.2, 3.0, 2.0, 0.4),
                sync,
                &state,
            );
            let b = render(
                &mut glided,
                0.01,
                offset_knobs(3.2, 3.0, 2.0, glide),
                sync,
                &state,
            );
            let c = render(
                &mut jumped,
                0.01,
                offset_knobs(3.2, 3.0, 2.0, jump),
                sync,
                &state,
            );
            if n >= 3000 {
                assert!(
                    (a - b).abs() < 1e-2,
                    "sync {sync}, glided, sample {n}: {a} vs {b}"
                );
                assert!(
                    (a - c).abs() < 1e-2,
                    "sync {sync}, jumped, sample {n}: {a} vs {c}"
                );
            }
        }
    }
}

/// A NaN or infinite phase offset holds the oscillator where it is.
#[test]
fn non_finite_phase_offset_holds() {
    let state = PhaseDistortionState::new(48000.0);
    let mut held = Voice::default();
    let mut glitched = Voice::default();
    for n in 0..200 {
        let v = match n {
            100 => f32::NAN,
            101 => f32::INFINITY,
            _ => 0.3,
        };
        let a = render(
            &mut held,
            0.01,
            offset_knobs(1.0, 3.0, 1.0, 0.3),
            true,
            &state,
        );
        let b = render(
            &mut glitched,
            0.01,
            offset_knobs(1.0, 3.0, 1.0, v),
            true,
            &state,
        );
        assert!((a - b).abs() < 1e-6, "sample {n}: {a} vs {b}");
    }
}

/// An offset moving at a steady speed plays as the frequency raised by that
/// speed, so modulating it across the wrap at a non-integer ratio moves the
/// slave as smoothly as the master, synced or free running.
#[test]
fn phase_offset_crossing_the_wrap_moves_smoothly() {
    let max = PhaseDistortionState::new(48000.0).max_frequency;
    let (master, ratio, speed) = (0.005, 3.2, 1e-4);
    for sync in [true, false] {
        let mut glided = PdOscillator::default();
        let mut sped = PdOscillator::default();
        glided.seed(0.9, ratio, master * ratio, 0.6, 2.5, max);
        sped.seed(0.9, ratio, master * ratio, 0.6, 2.5, max);
        let mut offset = 0.9f32;
        for n in 0..4000 {
            let moving = (500..2500).contains(&n);
            if moving {
                offset += speed;
            }
            let raise = if moving { speed / 2.0 } else { 0.0 };
            let a = glided.render(
                sync,
                master,
                master * ratio,
                0.6,
                2.5,
                offset.fract(),
                ratio,
                max,
            );
            let b = sped.render(
                sync,
                master + raise,
                (master + raise) * ratio,
                0.6,
                2.5,
                0.9,
                ratio,
                max,
            );
            for (a, b) in a.into_iter().zip(b) {
                assert!((a - b).abs() < 1e-3, "sync {sync}, sample {n}: {a} vs {b}");
            }
        }
    }
}

fn module(wait_for_zero_cross: bool) -> PhaseDistortion {
    let params = PhaseDistortionParams {
        freq: PolySignal::mono(Signal::Volts(-1.0)),
        ratio: None,
        amount: None,
        asymmetry: None,
        sync: true,
        fm: None,
        phase_offset: None,
        fm_mode: FmMode::default(),
        wait_for_zero_cross,
    };
    let mut outputs = PhaseDistortionOutputs::default();
    outputs.set_all_lanes(1);
    let mut module = PhaseDistortion {
        params,
        outputs,
        state: PhaseDistortionState::default(),
        lane_state: vec![Voice::default(); 1].into_boxed_slice(),
        _lane_count: 1,
        _channels: 1,
        _block_index: Default::default(),
        _connected: Default::default(),
    };
    module.init(48000.0);
    module
}

fn run(module: &mut PhaseDistortion, n: usize) -> Vec<f32> {
    (0..n)
        .map(|_| {
            module.update(48000.0);
            module.outputs.sample.get(0)
        })
        .collect()
}

/// A fresh voice is silent until its output first changes sign, then plays
/// exactly the unmuted render.
#[test]
fn fresh_voice_waits_for_a_zero_crossing() {
    let open = run(&mut module(false), 2000);
    let muted = run(&mut module(true), 2000);
    assert!(
        open[0].abs() > 3.0,
        "unmuted start {} sits at the peak",
        open[0]
    );
    let release = open
        .windows(2)
        .position(|w| (w[0] > 0.0) != (w[1] > 0.0) || w[1] == 0.0)
        .expect("the wave crosses zero")
        + 1;
    assert!(release > 1, "released at {release}");
    assert!(muted[..release].iter().all(|&v| v == 0.0));
    for (n, (m, o)) in muted.iter().zip(&open).enumerate().skip(release) {
        assert_eq!(m.to_bits(), o.to_bits(), "sample {n}");
    }
}

/// A fresh voice started part way through its cycle still waits for its
/// output to change sign, then plays exactly the unmuted render.
#[test]
fn offset_voice_waits_for_a_zero_crossing() {
    let with_offset = |wait| {
        let mut m = module(wait);
        m.params.phase_offset = Some(PolySignal::mono(Signal::Volts(0.3)));
        run(&mut m, 2000)
    };
    let open = with_offset(false);
    let muted = with_offset(true);
    assert_ne!(
        open[..50],
        run(&mut module(false), 50)[..],
        "the offset moves the start"
    );
    let release = open
        .windows(2)
        .position(|w| (w[0] > 0.0) != (w[1] > 0.0) || w[1] == 0.0)
        .expect("the wave crosses zero")
        + 1;
    assert!(muted[..release].iter().all(|&v| v == 0.0));
    for (n, (m, o)) in muted.iter().zip(&open).enumerate().skip(release) {
        assert_eq!(m.to_bits(), o.to_bits(), "sample {n}");
    }
}

/// A voice started at any offset starts on its waveform, so the mute opens
/// with a step no larger than the wave's own steps.
#[test]
fn offset_voice_opens_without_a_jump() {
    for n in 1..20 {
        let mut m = module(true);
        m.params.phase_offset = Some(PolySignal::mono(Signal::Volts(n as f32 * 0.05)));
        let out = run(&mut m, 2000);
        let largest = out[1000..]
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        let opening = out.iter().find(|&&v| v != 0.0).expect("the voice opens");
        assert!(
            opening.abs() <= largest,
            "offset {}: opened on {opening}, steps reach {largest}",
            n as f32 * 0.05
        );
    }
}

/// With the mute off, the module plays the engine's render from the first
/// sample.
#[test]
fn mute_off_plays_the_engine_render() {
    let state = PhaseDistortionState::new(48000.0);
    let mut voice = Voice::default();
    let f0 = voct_to_hz(-1.0) / 48000.0;
    let out = run(&mut module(false), 2000);
    for (n, actual) in out.iter().enumerate() {
        let expected = OUTPUT_GAIN
            * render(
                &mut voice,
                f0,
                knobs(
                    PhaseDistortionParams::RATIO_DEFAULT,
                    PhaseDistortionParams::AMOUNT_DEFAULT,
                    PhaseDistortionParams::ASYMMETRY_DEFAULT,
                ),
                true,
                &state,
            );
        assert_eq!(actual.to_bits(), expected.to_bits(), "sample {n}");
    }
}

/// A voice a patch update carries over keeps sounding without re-muting.
#[test]
fn carried_over_voice_does_not_re_mute() {
    let mut reference = module(true);
    let expected = run(&mut reference, 3000);

    let mut old = module(true);
    run(&mut old, 1000);
    let mut new = module(true);
    std::mem::swap(&mut new.state, &mut old.state);
    std::mem::swap(&mut new.lane_state, &mut old.lane_state);
    let after = run(&mut new, 2000);
    assert!(after[0] != 0.0);
    for (n, (a, e)) in after.iter().zip(&expected[1000..]).enumerate() {
        assert_eq!(a.to_bits(), e.to_bits(), "sample {n}");
    }
}
