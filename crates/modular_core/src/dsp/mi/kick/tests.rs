use super::analog::{Channel as KickChannel, KickState, Knobs as KickKnobs};
use super::pitch;
use super::synthetic::{Channel as SynthChannel, Knobs as SynthKnobs, SynthKickState};
use crate::dsp::mi::core::fixture::{max_error, records};
use crate::dsp::oscillators::FmMode;
use crate::poly::PolySignal;
use crate::types::{GlideMark, Signal};

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

/// `$mi.kick`'s knobs in volts: tone, decay, attack FM, self FM, drive and
/// accent.
fn kick_knobs([tone, decay, attack_fm, self_fm, drive, accent]: [f32; 6]) -> KickKnobs {
    KickKnobs {
        tone: (tone, mark()),
        decay: (decay, mark()),
        attack_fm: (attack_fm, mark()),
        self_fm: (self_fm, mark()),
        drive: (drive, mark()),
        accent: (accent, mark()),
    }
}

/// `$mi.synthKick`'s knobs in volts: tone, decay, dirt (`None` unpatched),
/// FM envelope, FM decay and accent.
fn synth_knobs(
    [tone, decay, fm_envelope, fm_decay, accent]: [f32; 5],
    dirt: Option<f32>,
) -> SynthKnobs {
    SynthKnobs {
        tone: (tone, mark()),
        decay: (decay, mark()),
        dirt: (dirt, mark()),
        fm_envelope: (fm_envelope, mark()),
        fm_decay: (fm_decay, mark()),
        accent: (accent, mark()),
    }
}

const KICK_DEFAULTS: [f32; 6] = [2.5, 2.5, 5.0, 5.0, 0.0, 4.0];
const SYNTH_DEFAULTS: [f32; 5] = [2.5, 2.5, 5.0, 0.0, 4.0];

/// Replays `kick.f32` (Plaits' two bass drums rendered a sample at a time):
/// strikes across accents and every knob's range, retriggers mid-decay and
/// at audio rate, a pitch swept by FM, high and near-1 Hz pitches, a drone
/// with the trigger unpatched, then strikes again.
#[test]
fn matches_plaits() {
    let kick_state = KickState::new(48000.0);
    let synth_state = SynthKickState::new(48000.0);
    let mut kick = KickChannel::default();
    let mut synth = SynthChannel::new(0x1234567, &synth_state.rates);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for r in records("kick", 12 + 2) {
        let trigger = (r[10] == 0.0).then_some(if r[11] != 0.0 { 5.0 } else { 0.0 });
        let dirt = (r[7] >= 0.0).then_some(r[7]);
        actual.push(kick.step(
            r[0],
            kick_knobs([r[1], r[2], r[4], r[5], r[6], r[3]]),
            trigger,
            &kick_state,
        ));
        actual.push(synth.step(
            r[0],
            synth_knobs([r[1], r[2], r[8], r[9], r[3]], dirt),
            trigger,
            &synth_state,
        ));
        expected.extend([r[12], r[13]]);
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(
        error < 1e-5,
        "max error {error} at sample {} ({})",
        at / 2,
        ["kick", "synthKick"][at % 2]
    );
}

/// `seconds` of `$mi.kick` at `hz`, struck once at the start.
fn kick_hit(sample_rate: f32, hz: f32, knobs: [f32; 6], seconds: f32) -> Vec<f32> {
    let state = KickState::new(sample_rate);
    let mut channel = KickChannel::default();
    (0..(seconds * sample_rate) as usize)
        .map(|n| {
            let trigger = if n == 0 { 5.0 } else { 0.0 };
            channel.step(hz / sample_rate, kick_knobs(knobs), Some(trigger), &state)
        })
        .collect()
}

/// `seconds` of `$mi.synthKick` at `hz` with noise from `seed`, struck once
/// after its noise filters settle from a fresh start.
fn synth_hit(
    sample_rate: f32,
    hz: f32,
    knobs: [f32; 5],
    dirt: Option<f32>,
    seed: u32,
    seconds: f32,
) -> Vec<f32> {
    let state = SynthKickState::new(sample_rate);
    let mut channel = SynthChannel::new(seed, &state.rates);
    let settle = (0.2 * sample_rate) as usize;
    (0..settle + (seconds * sample_rate) as usize)
        .map(|n| {
            let trigger = if n == settle { 5.0 } else { 0.0 };
            channel.step(
                hz / sample_rate,
                synth_knobs(knobs, dirt),
                Some(trigger),
                &state,
            )
        })
        .skip(settle)
        .collect()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// The part of `x` from `from` to `to` seconds.
fn window(x: &[f32], sample_rate: f32, from: f32, to: f32) -> &[f32] {
    &x[(from * sample_rate) as usize..(to * sample_rate) as usize]
}

/// The level in dB of `x` from `from` to `to` seconds.
fn level(x: &[f32], sample_rate: f32, from: f32, to: f32) -> f32 {
    20.0 * rms(window(x, sample_rate, from, to)).log10()
}

/// The strongest frequency within an octave either side of `hz`, in cents
/// from it, from `x` between `from` and `to` seconds.
fn pitch_cents(x: &[f32], sample_rate: f32, hz: f32, from: f32, to: f32) -> f32 {
    let x = window(x, sample_rate, from, to);
    (-1200..=1200)
        .step_by(2)
        .map(|cents| {
            let f = f64::from(hz) * 2f64.powf(f64::from(cents) / 1200.0);
            let w = std::f64::consts::TAU * f / f64::from(sample_rate);
            let (re, im) = x
                .iter()
                .enumerate()
                .fold((0.0f64, 0.0f64), |(re, im), (n, &v)| {
                    let phase = w * n as f64;
                    (
                        re + f64::from(v) * phase.cos(),
                        im + f64::from(v) * phase.sin(),
                    )
                });
            (cents, re * re + im * im)
        })
        .fold((0, 0.0), |best, c| if c.1 > best.1 { c } else { best })
        .0 as f32
}

/// Asserts the hits `render` gives at 44.1 and 96 kHz have the 48 kHz hit's
/// pitch, level and decay.
fn assert_rate_independent(name: &str, hz: f32, render: impl Fn(f32) -> Vec<f32>) {
    let measure = |sample_rate: f32| {
        let x = render(sample_rate);
        [
            pitch_cents(&x, sample_rate, hz, 0.02, 0.2),
            level(&x, sample_rate, 0.0, 0.05),
            level(&x, sample_rate, 0.05, 0.15) - level(&x, sample_rate, 0.15, 0.25),
        ]
    };
    let [cents, db, decay] = measure(48000.0);
    for sample_rate in [44100.0, 96000.0] {
        let [c, d, k] = measure(sample_rate);
        assert!(
            (c - cents).abs() <= 4.0,
            "{name} at {sample_rate} Hz: {c} cents vs {cents}"
        );
        assert!(
            (d - db).abs() < 0.3,
            "{name} at {sample_rate} Hz: {d} dB vs {db}"
        );
        assert!(
            (k - decay).abs() < 0.3,
            "{name} at {sample_rate} Hz: decays {k} dB vs {decay}"
        );
    }
}

/// A strike has the same pitch, level and decay at any rate.
#[test]
fn kick_sounds_the_same_at_any_rate() {
    for (hz, knobs) in [
        (50.0, KICK_DEFAULTS),
        (120.0, [4.0, 1.0, 5.0, 5.0, 3.0, 5.0]),
        (70.0, [1.0, 4.0, 2.0, 1.0, 0.0, 2.0]),
    ] {
        assert_rate_independent("kick", hz, |sample_rate| {
            kick_hit(sample_rate, hz, knobs, 0.3)
        });
    }
}

/// A strike's body has the same pitch, level and decay at any rate; with the
/// tone down there is no click or noise, and with no dirt no jitter.
#[test]
fn synth_kick_body_sounds_the_same_at_any_rate() {
    for (hz, knobs) in [
        (50.0, [0.0, 2.5, 5.0, 0.0, 4.0]),
        (90.0, [0.0, 4.0, 3.0, 5.0, 5.0]),
    ] {
        assert_rate_independent("synthKick", hz, |sample_rate| {
            synth_hit(sample_rate, hz, knobs, Some(0.0), 1, 0.3)
        });
    }
}

/// The click and noise of a bright strike are as loud at any rate, over many
/// noise draws.
#[test]
fn synth_kick_attack_as_loud_at_any_rate() {
    let attack = |sample_rate: f32| {
        let mut sum = 0.0;
        for seed in 0..16 {
            let x = synth_hit(
                sample_rate,
                50.0,
                [5.0, 2.5, 0.0, 0.0, 4.0],
                Some(5.0),
                seed,
                0.02,
            );
            sum += rms(&x);
        }
        sum / 16.0
    };
    let reference = attack(48000.0);
    for sample_rate in [44100.0, 96000.0, 192000.0] {
        let db = 20.0 * (attack(sample_rate) / reference).log10();
        assert!(db.abs() < 0.5, "{sample_rate} Hz: {db} dB from 48 kHz");
    }
}

/// How much of `x`'s energy is in its sample-to-sample change: higher for a
/// brighter or more distorted sound.
fn brightness(x: &[f32]) -> f32 {
    let change: f32 = x.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
    change / x.iter().map(|v| v * v).sum::<f32>()
}

/// How many times `x` crosses zero.
fn crossings(x: &[f32]) -> usize {
    x.windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// `$mi.kick` struck at 48 kHz with knob `index` at `volts`, the rest at
/// their defaults.
fn kick_with(hz: f32, index: usize, volts: f32) -> Vec<f32> {
    let mut knobs = KICK_DEFAULTS;
    knobs[index] = volts;
    kick_hit(48000.0, hz, knobs, 0.5)
}

/// `$mi.synthKick` struck at 48 kHz with knob `index` at `volts`, the rest at
/// their defaults.
fn synth_with(hz: f32, index: usize, volts: f32, dirt: Option<f32>) -> Vec<f32> {
    let mut knobs = SYNTH_DEFAULTS;
    knobs[index] = volts;
    synth_hit(48000.0, hz, knobs, dirt, 1, 0.5)
}

const SR: f32 = 48000.0;

/// Each `$mi.kick` knob moves the sound the way its description says.
#[test]
fn kick_knobs_do_what_they_say() {
    let [tone, decay, attack_fm, self_fm, drive, accent] = [0, 1, 2, 3, 4, 5];
    let (dull, bright) = (kick_with(50.0, tone, 0.0), kick_with(50.0, tone, 5.0));
    let (dull, bright) = (brightness(&dull), brightness(&bright));
    assert!(bright > 1.5 * dull, "tone: {bright} vs {dull}");

    let (short, long) = (kick_with(50.0, decay, 0.0), kick_with(50.0, decay, 5.0));
    let tail = |x: &[f32]| level(x, SR, 0.2, 0.4) - level(x, SR, 0.0, 0.1);
    assert!(tail(&long) > tail(&short) + 20.0, "decay");

    // A pitch jump in the first milliseconds.
    let attack = |volts| {
        let mut knobs = KICK_DEFAULTS;
        knobs[attack_fm] = volts;
        knobs[self_fm] = 0.0;
        crossings(window(&kick_hit(SR, 200.0, knobs, 0.05), SR, 0.0, 0.008))
    };
    assert!(attack(5.0) > attack(0.0) + 2, "attackFm");

    // Higher while loud, falling as the hit fades.
    let bend = |volts| {
        let mut knobs = KICK_DEFAULTS;
        knobs[attack_fm] = 0.0;
        knobs[self_fm] = volts;
        let x = kick_hit(SR, 60.0, knobs, 0.4);
        (
            pitch_cents(&x, SR, 60.0, 0.01, 0.06),
            pitch_cents(&x, SR, 60.0, 0.25, 0.4),
        )
    };
    let (flat, _) = bend(0.0);
    let (early, late) = bend(5.0);
    assert!(
        early > flat + 50.0,
        "selfFm raises the pitch: {early} vs {flat} cents"
    );
    assert!(
        early > late + 30.0,
        "selfFm bends down: {early} to {late} cents"
    );

    let (clean, driven) = (kick_with(50.0, drive, 0.0), kick_with(50.0, drive, 5.0));
    let (clean, driven) = (brightness(&clean), brightness(&driven));
    assert!(driven > 1.5 * clean, "drive: {driven} vs {clean}");
    // Gone by 3 kHz.
    assert_eq!(kick_with(3000.0, drive, 0.0), kick_with(3000.0, drive, 5.0));

    let (soft, hard) = (kick_with(50.0, accent, 0.0), kick_with(50.0, accent, 5.0));
    assert!(peak(&hard) > 1.5 * peak(&soft), "accent");
}

/// Each `$mi.synthKick` knob moves the sound the way its description says.
#[test]
fn synth_kick_knobs_do_what_they_say() {
    let [tone, decay, fm_envelope, fm_decay, accent] = [0, 1, 2, 3, 4];
    let (dull, bright) = (
        synth_with(50.0, tone, 0.0, None),
        synth_with(50.0, tone, 5.0, None),
    );
    let (dull, bright) = (brightness(&dull), brightness(&bright));
    assert!(bright > 2.0 * dull, "tone: {bright} vs {dull}");

    let (short, long) = (
        synth_with(50.0, decay, 0.0, None),
        synth_with(50.0, decay, 5.0, None),
    );
    let tail = |x: &[f32]| level(x, SR, 0.2, 0.4) - level(x, SR, 0.0, 0.1);
    assert!(tail(&long) > tail(&short) + 20.0, "decay");

    // Jitter: the drone's period wanders.
    let jitter = |dirt| {
        let state = SynthKickState::new(SR);
        let mut channel = SynthChannel::new(1, &state.rates);
        let x: Vec<f32> = (0..48000)
            .map(|_| {
                channel.step(
                    100.0 / SR,
                    synth_knobs([2.5, 5.0, 5.0, 0.0, 4.0], Some(dirt)),
                    None,
                    &state,
                )
            })
            .collect();
        let mean = x.iter().sum::<f32>() / x.len() as f32;
        let rises: Vec<f32> = x
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] < mean && w[1] >= mean)
            .map(|(n, w)| n as f32 + (mean - w[0]) / (w[1] - w[0]))
            .collect();
        let periods: Vec<f32> = rises.windows(2).skip(10).map(|w| w[1] - w[0]).collect();
        let average = periods.iter().sum::<f32>() / periods.len() as f32;
        rms(&periods.iter().map(|p| p - average).collect::<Vec<_>>())
    };
    let (clean, gritty) = (jitter(0.0), jitter(5.0));
    assert!(
        gritty > 10.0 * clean,
        "dirt: {gritty} vs {clean} samples of jitter"
    );
    // Gone by 6 kHz.
    assert_eq!(
        synth_with(6000.0, tone, 2.5, Some(0.0)),
        synth_with(6000.0, tone, 2.5, Some(5.0))
    );
    // Unpatched, cleaner as the decay lengthens: 2 V at no decay.
    assert_eq!(
        synth_with(80.0, decay, 0.0, None),
        synth_with(80.0, decay, 0.0, Some(2.0))
    );

    let sweep = |envelope| {
        crossings(window(
            &synth_with(100.0, fm_envelope, envelope, None),
            SR,
            0.0,
            0.02,
        ))
    };
    assert!(sweep(5.0) > sweep(0.0) + 2, "fmEnvelope");

    let lasting = |volts| {
        crossings(window(
            &synth_with(100.0, fm_decay, volts, None),
            SR,
            0.02,
            0.06,
        ))
    };
    assert!(lasting(5.0) > lasting(0.0) + 2, "fmDecay");

    let (soft, hard) = (
        synth_with(50.0, accent, 0.0, None),
        synth_with(50.0, accent, 5.0, None),
    );
    assert!(peak(&hard) > 1.5 * peak(&soft), "accent");
}

/// With the trigger unpatched each drum drones at its pitch, as loud as accent
/// and decay allow: silent with no decay.
#[test]
fn drones_when_unpatched() {
    let kick_state = KickState::new(SR);
    let synth_state = SynthKickState::new(SR);
    let drone = |decay: f32| {
        let mut kick = KickChannel::default();
        let mut synth = SynthChannel::new(1, &synth_state.rates);
        // No self FM, which bends the drone's pitch.
        let mut knobs = KICK_DEFAULTS;
        knobs[1] = decay;
        knobs[3] = 0.0;
        let mut synth_knobs_volts = SYNTH_DEFAULTS;
        synth_knobs_volts[1] = decay;
        let (k, s): (Vec<f32>, Vec<f32>) = (0..24000)
            .map(|_| {
                (
                    kick.step(80.0 / SR, kick_knobs(knobs), None, &kick_state),
                    synth.step(
                        80.0 / SR,
                        synth_knobs(synth_knobs_volts, Some(0.0)),
                        None,
                        &synth_state,
                    ),
                )
            })
            .unzip();
        (k, s)
    };
    let (kick, synth) = drone(4.0);
    for (name, x) in [("kick", &kick), ("synthKick", &synth)] {
        let tail = window(x, SR, 0.25, 0.5);
        let ac: Vec<f32> = {
            let mean = tail.iter().sum::<f32>() / tail.len() as f32;
            tail.iter().map(|v| v - mean).collect()
        };
        assert!(rms(&ac) > 0.05, "{name} drones: {}", rms(&ac));
        let cents = pitch_cents(x, SR, 80.0, 0.25, 0.5);
        assert!(cents.abs() < 150.0, "{name} drones at {cents} cents");
    }
    let (kick, synth) = drone(0.0);
    assert!(
        rms(window(&kick, SR, 0.25, 0.5)) < 1e-4,
        "kick silent with no decay"
    );
    let tail = window(&synth, SR, 0.25, 0.5);
    let swing =
        tail.iter().fold(f32::MIN, |m, &v| m.max(v)) - tail.iter().fold(f32::MAX, |m, &v| m.min(v));
    assert!(swing < 1e-4, "synthKick silent with no decay");
}

/// A trigger held high while it is unpatched strikes once patched again.
#[test]
fn trigger_patched_again_rises() {
    let kick_state = KickState::new(SR);
    let synth_state = SynthKickState::new(SR);
    let run = |unpatch: bool| {
        let mut kick = KickChannel::default();
        let mut synth = SynthChannel::new(1, &synth_state.rates);
        let mut out = (Vec::new(), Vec::new());
        for n in 0..30000 {
            let trigger = if unpatch && (10000..12000).contains(&n) {
                None
            } else {
                Some(5.0)
            };
            out.0.push(kick.step(
                50.0 / SR,
                kick_knobs([2.5, 0.5, 5.0, 5.0, 0.0, 4.0]),
                trigger,
                &kick_state,
            ));
            out.1.push(synth.step(
                50.0 / SR,
                synth_knobs([2.5, 0.5, 5.0, 0.0, 4.0], None),
                trigger,
                &synth_state,
            ));
        }
        out
    };
    let (held_kick, held_synth) = run(false);
    let (kick, synth) = run(true);
    for (name, held, repatched) in [("kick", held_kick, kick), ("synthKick", held_synth, synth)] {
        let after = |x: &[f32]| peak(&x[12000..14000]);
        assert!(
            after(&repatched) > 10.0 * after(&held),
            "{name}: {} vs {}",
            after(&repatched),
            after(&held)
        );
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let kick_state = KickState::new(SR);
    let synth_state = SynthKickState::new(SR);
    let mut kicks = [KickChannel::default(), KickChannel::default()];
    let mut synths = [
        SynthChannel::new(7, &synth_state.rates),
        SynthChannel::new(7, &synth_state.rates),
    ];
    for n in 0..8000 {
        let v = match n {
            500 => f32::NAN,
            501 => f32::INFINITY,
            502 => f32::NEG_INFINITY,
            _ => 2.0,
        };
        let trigger = Some(if n % 1000 == 0 { 5.0 } else { 0.0 });
        let a = kicks[0].step(0.002, kick_knobs([2.0; 6]), trigger, &kick_state);
        let b = kicks[1].step(0.002, kick_knobs([v; 6]), trigger, &kick_state);
        assert_eq!(a, b, "kick sample {n}");
        let a = synths[0].step(
            0.002,
            synth_knobs([2.0; 5], Some(2.0)),
            trigger,
            &synth_state,
        );
        let b = synths[1].step(0.002, synth_knobs([v; 5], Some(v)), trigger, &synth_state);
        assert_eq!(a, b, "synthKick sample {n}");
    }
}

/// A drum rings at the magnitude of a negative frequency: through-zero FM
/// matches linear FM wherever linear FM keeps the frequency positive, and
/// past that sounds the frequency's magnitude rather than stopping.
#[test]
fn through_zero_fm_rings_at_the_magnitude() {
    let inv = 1.0 / SR;
    for fm in [-0.9, -0.3, 0.0, 0.7, 2.5] {
        assert_eq!(
            pitch(-1.0, fm, FmMode::ThroughZero, inv),
            pitch(-1.0, fm, FmMode::Lin, inv)
        );
    }
    assert_eq!(
        pitch(-1.0, -3.0, FmMode::ThroughZero, inv),
        pitch(-1.0, 1.0, FmMode::Lin, inv)
    );
}

/// NaN, infinite and extreme pitches, knob jumps, unpatching and audio-rate
/// triggers at any engine rate leave both drums finite and bounded.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 44100.0, 192000.0] {
        let kick_state = KickState::new(sample_rate);
        let synth_state = SynthKickState::new(sample_rate);
        let mut kick = KickChannel::default();
        let mut synth = SynthChannel::new(3, &synth_state.rates);
        let mut peak = 0.0f32;
        for n in 0..40000 {
            let freq = match n % 7 {
                0 => f32::NAN,
                1 => f32::INFINITY,
                2 => f32::NEG_INFINITY,
                3 => 12.0,
                4 => -12.0,
                _ => (n as f32 * 0.001).sin() * 4.0,
            };
            let f0 = pitch(freq, 0.0, FmMode::Exp, 1.0 / sample_rate);
            let knob = (n % 11) as f32 * 0.5;
            let trigger = match n % 20000 {
                0..5000 => Some(if n % 2 == 0 { 5.0 } else { 0.0 }),
                5000..15000 => Some(if n % 997 == 0 { 5.0 } else { 0.0 }),
                _ => None,
            };
            let k = kick.step(
                f0,
                kick_knobs([knob, 5.0 - knob, knob, 5.0, knob, 5.0]),
                trigger,
                &kick_state,
            );
            let s = synth.step(
                f0,
                synth_knobs([knob, 5.0 - knob, knob, 5.0 - knob, 5.0], Some(knob)),
                trigger,
                &synth_state,
            );
            assert!(
                k.is_finite() && s.is_finite(),
                "{sample_rate} Hz, sample {n}"
            );
            peak = peak.max(s.abs());
        }
        // The overdrive bounds the kick by construction. The synthetic kick
        // clips at ±5 V past 1.25, as Plaits' does; nothing in it grows.
        assert!(peak < 10.0, "{sample_rate} Hz: synthKick peak {peak}");
    }
}
