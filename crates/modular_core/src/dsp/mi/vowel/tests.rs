use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(model: f32, vowel: f32, formant_shift: f32) -> Knobs {
    Knobs {
        model: (model, mark()),
        vowel: (vowel, mark()),
        formant_shift: (formant_shift, mark()),
    }
}

/// A channel at `rates`, seeded as `vowel.cc` seeds stmlib's generator.
fn channel(rates: &Rates) -> Channel {
    Channel {
        random: Random::new(0x21),
        seeded: true,
        ..Channel::new(rates)
    }
}

/// Replays `vowel.f32` (Plaits' naive, SAM and LPC synths blended as its
/// speech engine blends them, then its limiter): every model and blend,
/// clicks and consonants, vowel and formant sweeps, and pitches past SAM's
/// and the LPC synth's limits.
#[test]
fn matches_plaits() {
    let rates = Rates::new(48000.0);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for records in records("vowel", 5 + 2).chunks(12000) {
        let mut ch = channel(&rates);
        for record in records {
            let (out, excitation) = ch.step(
                record[0],
                knobs(record[1], record[2], record[3]),
                Some(record[4]),
                &rates,
            );
            actual.extend([out, excitation]);
            expected.extend([record[5], record[6]]);
        }
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// `samples` of a steady voice at `hz`: the output and the excitation, as
/// pairs. A trigger rises on each sample in `triggers`.
fn render(
    sample_rate: f32,
    hz: f32,
    [model, vowel, formant_shift]: [f32; 3],
    samples: usize,
    triggers: &[usize],
) -> Vec<(f32, f32)> {
    let rates = Rates::new(sample_rate);
    let mut ch = channel(&rates);
    (0..samples)
        .map(|n| {
            let trigger = if triggers.contains(&n) { 5.0 } else { 0.0 };
            ch.step(
                hz / sample_rate,
                knobs(model, vowel, formant_shift),
                Some(trigger),
                &rates,
            )
        })
        .collect()
}

fn outputs(x: &[(f32, f32)]) -> Vec<f32> {
    x.iter().map(|&(out, _)| out).collect()
}

/// Each model and a blend have the same harmonics at 44.1 and 96 kHz as at
/// 48 kHz, in the voice and in its excitation.
#[test]
fn sounds_the_same_at_any_rate() {
    let hz = 110.0;
    for (knobs, tolerance) in [
        ([0.0, 1.0, 2.5], 0.03),
        ([0.0, 4.0, 4.0], 0.03),
        ([2.5, 1.5, 1.0], 0.02),
        ([2.5, 4.0, 4.5], 0.02),
        ([5.0, 0.5, 2.5], 0.03),
        ([5.0, 3.0, 0.5], 0.03),
        ([3.75, 2.0, 3.0], 0.05),
    ] {
        assert_rate_independent(
            |sample_rate| {
                outputs(&render(
                    sample_rate,
                    hz,
                    knobs,
                    (0.5 * sample_rate) as usize,
                    &[],
                ))
            },
            hz,
            tolerance,
        );
        assert_rate_independent(
            |sample_rate| {
                render(sample_rate, hz, knobs, (0.5 * sample_rate) as usize, &[])
                    .iter()
                    .map(|&(_, excitation)| excitation)
                    .collect()
            },
            hz,
            tolerance,
        );
    }
}

/// A SAM consonant lasts 50 ms at any rate: a triggered voice rejoins the
/// untriggered one within a glottal period of its end.
#[test]
fn consonants_last_the_same_at_any_rate() {
    let hz = 200.0;
    for sample_rate in [44100.0, 48000.0, 96000.0] {
        let samples = (0.2 * sample_rate) as usize;
        let start = (0.05 * sample_rate) as usize;
        let knobs = [2.5, 1.0, 2.0];
        let plain = render(sample_rate, hz, knobs, samples, &[]);
        let triggered = render(sample_rate, hz, knobs, samples, &[start]);
        let last = (0..samples)
            .rev()
            .find(|&n| (plain[n].0 - triggered[n].0).abs() > 1e-4)
            .unwrap();
        let seconds = (last - start) as f32 / sample_rate;
        assert!(
            (0.049..0.05 + 1.0 / hz + 1e-3).contains(&seconds),
            "{sample_rate} Hz: the consonant lasts {seconds} s"
        );
    }
}

/// Through-zero FM runs the glottis backward. SAM's sine formants restart
/// where a forward period ends, so its backward voice is its forward one
/// reversed; the filtered voices keep their spectrum.
#[test]
fn runs_backward() {
    let sample_rate = 48000.0;
    let f0 = 1.0 / 256.0;
    let samples = 24000;
    let render = |f0: f32, model: f32| {
        let rates = Rates::new(sample_rate);
        let mut ch = channel(&rates);
        (0..samples)
            .map(|_| ch.step(f0, knobs(model, 1.5, 2.0), None, &rates).0)
            .collect::<Vec<_>>()
    };

    let forward = render(f0, 2.5);
    let backward = render(-f0, 2.5);
    let peak = forward.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    // Past the limiter's settling, backward sample k retraces forward
    // sample n - k, n a whole number of periods.
    let n = 20478;
    let error = (12000..n)
        .map(|k| (backward[k] - forward[n - k]).abs())
        .fold(0.0f32, f32::max);
    assert!(peak > 0.1, "SAM: silent");
    assert!(error < 1e-3 * peak, "SAM: mirror error {error} of {peak}");

    let hz = f0 * sample_rate;
    for model in [0.0, 5.0] {
        let forward = harmonics(&render(f0, model), hz, sample_rate, 24);
        let backward = harmonics(&render(-f0, model), hz, sample_rate, 24);
        let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
        for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
            assert!(
                (b - f).abs() < 0.05 * loudest,
                "model {model}, harmonic {}: {b} backward vs {f}",
                h + 1
            );
        }
    }
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let rates = Rates::new(48000.0);
    let mut held = channel(&rates);
    let mut glitched = channel(&rates);
    for n in 0..8000 {
        let v = match n % 2000 {
            500 => f32::NAN,
            501 => f32::INFINITY,
            502 => f32::NEG_INFINITY,
            _ => 1.0 + (n / 2000) as f32,
        };
        let steady = 1.0 + (n / 2000) as f32;
        let trigger = Some(if n % 1500 == 3 { 5.0 } else { 0.0 });
        let a = held.step(0.004, knobs(steady, steady, steady), trigger, &rates);
        let b = glitched.step(0.004, knobs(v, v, v), trigger, &rates);
        assert_eq!(a, b, "sample {n}");
    }
}

/// Any knobs, pitches (NaN and infinite among them), triggers and knob jumps
/// leave both outputs finite at any engine rate. The limiter's slow release
/// lets a jump overshoot full scale a little, as in Plaits; the output stage
/// clips it.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 22050.0, 44100.0, 96000.0, 192000.0] {
        let rates = Rates::new(sample_rate);
        let mut ch = channel(&rates);
        for n in 0..40000 {
            let f0 = match n % 600 {
                100 => f32::NAN,
                150 => f32::INFINITY,
                151 => f32::NEG_INFINITY,
                200..=260 => 0.5,
                300..=360 => -0.5,
                400..=430 => 0.0,
                _ => {
                    ((n as f32 * 0.0007).sin() * 3.0).exp2() * 220.0 / sample_rate
                        * if n % 5000 < 2500 { 1.0 } else { -1.0 }
                }
            };
            let t = n as f32 * 0.0011;
            let jump = if n % 977 < 100 { 5.0 } else { 0.0 };
            let knobs = knobs(
                2.5 + 2.6 * (t * 0.3).sin(),
                jump + 2.5 * (t * 1.7).sin(),
                2.5 + 3.0 * (t * 0.7).sin(),
            );
            let trigger = Some(if n % 701 == 0 { 5.0 } else { 0.0 });
            let (out, excitation) = ch.step(f0, knobs, trigger, &rates);
            assert!(
                out.abs() < 1.5 && excitation.abs() < 3.0,
                "{sample_rate} Hz, sample {n}: {out}, {excitation}"
            );
        }
    }
}

/// `model` sounds the formant-filter voice alone at 0, SAM alone at 2.5 and
/// the LPC voice alone at 5.
#[test]
fn model_runs_from_formant_filters_through_sam_to_lpc() {
    let rates = Rates::new(48000.0);
    let f0 = 130.0 / 48000.0;
    let (vowel, formant_shift) = (knob(1.5), knob(3.0));
    for (model, synth) in [(0.0, 0), (2.5, 1), (5.0, 2)] {
        let mut ch = channel(&rates);
        let mut naive = NaiveSynth::new(&rates);
        let mut sam = SamSynth::default();
        let mut lpc = LpcVowels::default();
        let mut random = Random::new(0x21);
        let mut limiter = Limiter::default();
        let mut error = 0.0f32;
        for _ in 0..4800 {
            let (out, excitation) = ch.step(f0, knobs(model, 1.5, 3.0), None, &rates);
            let (alone_excitation, alone_out) = match synth {
                0 => naive.step(false, f0, vowel, formant_shift, &rates),
                1 => sam.step(false, f0, vowel, formant_shift, &rates),
                _ => {
                    let [e, o] = lpc.step(
                        false,
                        f0 * rates.lpc_pitch_scale,
                        vowel,
                        formant_shift,
                        &mut random,
                        &rates,
                    );
                    (e, o)
                }
            };
            let alone_out = limiter.process(alone_out * PRE_GAIN, &rates.limiter);
            error = error
                .max((out - alone_out).abs())
                .max((excitation - alone_excitation).abs());
        }
        assert!(error < 1e-6, "model {model}: {error} from its synth alone");
    }
}

/// The amplitude-weighted mean frequency of the harmonics of `hz` between
/// 150 Hz and 1.2 kHz: where the first formant sits.
fn first_formant(x: &[f32], hz: f32, sample_rate: f32) -> f32 {
    let amplitudes = harmonics(x, hz, sample_rate, (1200.0 / hz) as usize);
    let (weighted, total) = amplitudes
        .iter()
        .enumerate()
        .map(|(h, a)| ((h + 1) as f32 * hz, a * a))
        .filter(|&(f, _)| f >= 150.0)
        .fold((0.0, 0.0), |(w, t), (f, p)| (w + f * p, t + p));
    weighted / total
}

/// `vowel` runs from an open a (0), its first formant high, to a closed u
/// (5), its first formant low, on every model; `formantShift` raises the
/// formants from 0 to 5.
#[test]
fn vowel_and_formant_shift_move_the_formants_as_described() {
    let sample_rate = 48000.0;
    let hz = 100.0;
    let formant = |knobs: [f32; 3]| {
        first_formant(
            &outputs(&render(sample_rate, hz, knobs, 24000, &[])),
            hz,
            sample_rate,
        )
    };
    for model in [0.0, 2.5, 5.0] {
        let (a, u) = (formant([model, 0.0, 2.5]), formant([model, 5.0, 2.5]));
        assert!(
            a > 1.25 * u,
            "model {model}: first formant {a} Hz at a, {u} Hz at u"
        );
        let (deep, small) = (formant([model, 0.0, 0.0]), formant([model, 0.0, 5.0]));
        assert!(
            small > 1.15 * deep,
            "model {model}: first formant {deep} Hz at shift 0, {small} Hz at 5"
        );
    }
}

/// Unpatching the trigger forgets its last state, so a trigger patched again
/// while high still rises.
#[test]
fn a_trigger_patched_again_rises() {
    let rates = Rates::new(48000.0);
    let mut ch = channel(&rates);
    for _ in 0..100 {
        ch.step(0.004, knobs(2.5, 2.5, 2.5), Some(5.0), &rates);
    }
    ch.step(0.004, knobs(2.5, 2.5, 2.5), None, &rates);
    assert!(ch.trigger.process(5.0));
}

/// A trigger changes each model's sound for the syllable's start: a pitch
/// drop on the formant filters, a consonant on SAM and the LPC voice.
#[test]
fn a_trigger_starts_a_syllable() {
    let sample_rate = 48000.0;
    let start = 4800;
    let window = start..start + 2400;
    for model in [0.0, 2.5, 5.0] {
        let knobs = [model, 1.0, 2.5];
        let plain = outputs(&render(sample_rate, 130.0, knobs, 9600, &[]));
        let triggered = outputs(&render(sample_rate, 130.0, knobs, 9600, &[start]));
        let level: f32 = window.clone().map(|n| plain[n] * plain[n]).sum();
        let change: f32 = window
            .clone()
            .map(|n| (triggered[n] - plain[n]).powi(2))
            .sum();
        assert!(
            change > 0.1 * level,
            "model {model}: the trigger changes {change} of {level}"
        );
        assert_eq!(plain[..start], triggered[..start], "model {model}");
    }
}
