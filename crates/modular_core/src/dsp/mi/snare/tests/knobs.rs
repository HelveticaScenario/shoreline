use super::{NO_SNARES, SR, SynthChannel, SynthSnareState, snare_hit, synth_hit, synth_knobs};
use crate::dsp::mi::drum::measure::{crossings, level, peak, window};

/// `$mi.snare` struck at 48 kHz at `hz` with knob `index` at `volts`, the
/// rest at `knobs`.
fn snare_with(hz: f32, knobs: [f32; 4], index: usize, volts: f32) -> Vec<f32> {
    let mut knobs = knobs;
    knobs[index] = volts;
    snare_hit(SR, hz, knobs, 1, 0.5)
}

/// `$mi.synthSnare` struck at 48 kHz at `hz` with knob `index` at `volts`,
/// the rest at `knobs`.
fn synth_with(hz: f32, knobs: [f32; 4], index: usize, volts: f32) -> Vec<f32> {
    let mut knobs = knobs;
    knobs[index] = volts;
    synth_hit(SR, hz, knobs, 1, 0.5)
}

/// How fast `x` fades: its level from 0.2 to 0.4 s against its first 0.1 s.
fn tail(x: &[f32]) -> f32 {
    level(x, SR, 0.2, 0.4) - level(x, SR, 0.0, 0.1)
}

/// The amplitude of `x` at `hz`, through a Hann window.
fn amplitude(x: &[f32], hz: f32) -> f32 {
    let w = std::f64::consts::TAU * f64::from(hz) / f64::from(SR);
    let len = x.len() as f64;
    let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (n, &v)| {
        let phase = w * n as f64;
        let v = f64::from(v) * (0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / len).cos());
        (re + v * phase.cos(), im + v * phase.sin())
    });
    (4.0 * (re * re + im * im).sqrt() / len) as f32
}

/// Each knob's index: tone and attack FM come first, on their own drums.
const TONE: usize = 0;
const ATTACK_FM: usize = 0;
const DECAY: usize = 1;
const ACCENT: usize = 3;

/// Each `$mi.snare` knob moves the sound the way its description says.
#[test]
fn snare_knobs_do_what_they_say() {
    // Two tones, the higher one coming up, then overtones.
    let hz = 200.0;
    let modes = |volts| {
        let x = snare_with(hz, NO_SNARES, TONE, volts);
        let x = window(&x, SR, 0.0, 0.2);
        [1.0, 2.0, 3.18].map(|ratio| amplitude(x, hz * ratio))
    };
    let (low, high, full) = (modes(0.0), modes(3.3), modes(5.0));
    assert!(
        high[1] / high[0] > 4.0 * low[1] / low[0],
        "tone 3.3: {high:?} vs {low:?}"
    );
    assert!(full[2] > 10.0 * high[2], "tone 5: {full:?} vs {high:?}");

    let (short, long) = (
        snare_with(hz, NO_SNARES, DECAY, 0.0),
        snare_with(hz, NO_SNARES, DECAY, 5.0),
    );
    assert!(tail(&long) > tail(&short) + 20.0, "decay: shell");
    let snares = [2.5, 2.5, 5.0, 4.0];
    let (short, long) = (
        snare_with(hz, snares, DECAY, 0.0),
        snare_with(hz, snares, DECAY, 5.0),
    );
    assert!(tail(&long) > tail(&short) + 20.0, "decay: snares");

    // All shell, without noise; all snares, without the shell.
    let snare_seeded = |seed, knobs| snare_hit(SR, hz, knobs, seed, 0.2);
    assert_eq!(snare_seeded(1, NO_SNARES), snare_seeded(2, NO_SNARES));
    assert_ne!(snare_seeded(1, snares), snare_seeded(2, snares));
    let (bare, struck) = (
        snare_with(hz, snares, TONE, 0.0),
        snare_with(hz, snares, TONE, 5.0),
    );
    assert_eq!(bare, struck, "snappy 5 has no shell");

    let (soft, hard) = (
        snare_with(hz, NO_SNARES, ACCENT, 0.0),
        snare_with(hz, NO_SNARES, ACCENT, 5.0),
    );
    assert!(peak(&hard) > 1.5 * peak(&soft), "accent");
    let (soft, hard) = (
        snare_with(hz, snares, ACCENT, 0.0),
        snare_with(hz, snares, ACCENT, 5.0),
    );
    assert_eq!(soft, hard, "accent leaves the snares");
}

/// Each `$mi.synthSnare` knob moves the sound the way its description says.
#[test]
fn synth_snare_knobs_do_what_they_say() {
    // A pitch sweep in the first milliseconds, and a longer body.
    let hz = 200.0;
    let (plain, swept) = (
        synth_with(hz, NO_SNARES, ATTACK_FM, 0.0),
        synth_with(hz, NO_SNARES, ATTACK_FM, 5.0),
    );
    let early = |x: &[f32]| crossings(window(x, SR, 0.0, 0.006));
    assert!(early(&swept) > early(&plain) + 2, "attackFm sweeps");
    assert!(tail(&swept) > tail(&plain) + 6.0, "attackFm lengthens");
    // On low notes at high settings, the oscillators' resets intermodulate
    // them: a component at the difference of their frequencies.
    let drone = |hz: f32, attack_fm| {
        let state = SynthSnareState::new(SR);
        let mut channel = SynthChannel::new(1);
        let knobs = [attack_fm, 4.0, 0.0, 4.0];
        (0..24000)
            .map(|_| channel.step(hz / SR, synth_knobs(knobs), None, &state))
            .collect::<Vec<f32>>()
    };
    let difference = |attack_fm| amplitude(&drone(100.0, attack_fm)[4800..], 47.0);
    let (clean, rough) = (difference(0.0), difference(5.0));
    assert!(
        rough > 10.0 * clean,
        "attackFm roughens: {rough} vs {clean}"
    );
    assert_eq!(drone(5000.0, 0.0), drone(5000.0, 5.0));
    assert_eq!(drone(1000.0, 0.0), drone(1000.0, 1.5));

    let (short, long) = (
        synth_with(hz, NO_SNARES, DECAY, 0.0),
        synth_with(hz, NO_SNARES, DECAY, 5.0),
    );
    assert!(tail(&long) > tail(&short) + 20.0, "decay: body");
    let snares = [2.5, 2.5, 5.0, 4.0];
    let (short, long) = (
        synth_with(hz, snares, DECAY, 0.0),
        synth_with(hz, snares, DECAY, 5.0),
    );
    assert!(tail(&long) > tail(&short) + 20.0, "decay: snares");

    // All body, without noise; all snares, without the body.
    let synth_seeded = |seed, knobs| synth_hit(SR, hz, knobs, seed, 0.2);
    assert_eq!(synth_seeded(1, NO_SNARES), synth_seeded(2, NO_SNARES));
    assert_ne!(synth_seeded(1, snares), synth_seeded(2, snares));
    assert_eq!(
        synth_with(hz, snares, ATTACK_FM, 0.0),
        synth_with(hz, snares, ATTACK_FM, 5.0),
        "snappy 5 has no body"
    );

    let (soft, hard) = (
        synth_with(hz, NO_SNARES, ACCENT, 0.0),
        synth_with(hz, NO_SNARES, ACCENT, 5.0),
    );
    assert!(peak(&hard) > 1.5 * peak(&soft), "accent");
}
