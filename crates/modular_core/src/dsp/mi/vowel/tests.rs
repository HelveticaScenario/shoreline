use super::lpc::LpcVowels;
use super::naive::NaiveSynth;
use super::sam::SamSynth;
use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::mi::core::random::Random;
use crate::poly::PolySignal;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

fn knobs(vowel: f32, formant_shift: f32) -> Knobs {
    Knobs {
        vowel: (vowel, mark()),
        formant_shift: (formant_shift, mark()),
    }
}

/// A voice as `vowel.cc` starts it, its noise seeded as the driver seeds
/// stmlib's generator.
trait TestVoice: Voice {
    const NAME: &'static str;
    fn channel(rates: &Rates) -> Channel<Self> {
        Channel::new(rates)
    }
}

impl TestVoice for NaiveSynth {
    const NAME: &'static str = "formant filters";
}

impl TestVoice for SamSynth {
    const NAME: &'static str = "SAM";
}

impl TestVoice for LpcVowels {
    const NAME: &'static str = "LPC";
    fn channel(rates: &Rates) -> Channel<Self> {
        let mut channel = Channel::<Self>::new(rates);
        channel.voice.random = Random::new(0x21);
        channel.voice.seeded = true;
        channel
    }
}

/// Runs `test` for each voice.
macro_rules! each_voice {
    ($test:ident) => {
        $test::<NaiveSynth>();
        $test::<SamSynth>();
        $test::<LpcVowels>();
    };
}

/// Replays the cases of `vowel.f32` for synth `index` (Plaits' naive, SAM or
/// LPC synth alone, then its limiter): vowel and formant sweeps, clicks and
/// consonants, and pitches past the synth's limits.
fn replay<V: TestVoice>(index: f32) {
    let rates = Rates::new(48000.0);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for records in records("vowel", 5 + 2).chunks(12000) {
        if records[0][0] != index {
            continue;
        }
        let mut ch = V::channel(&rates);
        for record in records {
            let (out, excitation) = ch.step(
                record[1],
                knobs(record[2], record[3]),
                Some(record[4]),
                &rates,
            );
            actual.extend([out, excitation]);
            expected.extend([record[5], record[6]]);
        }
    }
    assert!(!actual.is_empty(), "{}: no cases", V::NAME);
    let (error, at) = max_error(&actual, &expected);
    assert!(
        error < 1e-5,
        "{}: max error {error} at sample {}",
        V::NAME,
        at / 2
    );
}

#[test]
fn matches_plaits() {
    replay::<NaiveSynth>(0.0);
    replay::<SamSynth>(1.0);
    replay::<LpcVowels>(2.0);
}

/// `samples` of a steady voice at `hz`: the output and the excitation, as
/// pairs. A trigger rises on each sample in `triggers`.
fn render<V: TestVoice>(
    sample_rate: f32,
    hz: f32,
    [vowel, formant_shift]: [f32; 2],
    samples: usize,
    triggers: &[usize],
) -> Vec<(f32, f32)> {
    let rates = Rates::new(sample_rate);
    let mut ch = V::channel(&rates);
    (0..samples)
        .map(|n| {
            let trigger = if triggers.contains(&n) { 5.0 } else { 0.0 };
            ch.step(
                hz / sample_rate,
                knobs(vowel, formant_shift),
                Some(trigger),
                &rates,
            )
        })
        .collect()
}

fn outputs(x: &[(f32, f32)]) -> Vec<f32> {
    x.iter().map(|&(out, _)| out).collect()
}

/// Each voice has the same harmonics at 44.1 and 96 kHz as at 48 kHz, in the
/// voice and in its excitation.
#[test]
fn sounds_the_same_at_any_rate() {
    fn check<V: TestVoice>() {
        let hz = 110.0;
        for knobs in [[1.0, 2.5], [4.0, 4.0], [0.5, 0.5], [3.0, 1.0]] {
            let render = |sample_rate: f32| {
                render::<V>(sample_rate, hz, knobs, (0.5 * sample_rate) as usize, &[])
            };
            assert_rate_independent(|sample_rate| outputs(&render(sample_rate)), hz, 0.03);
            assert_rate_independent(
                |sample_rate| render(sample_rate).iter().map(|&(_, e)| e).collect(),
                hz,
                0.03,
            );
        }
    }
    each_voice!(check);
}

/// A SAM consonant lasts 50 ms at any rate: a triggered voice rejoins the
/// untriggered one within a glottal period of its end.
#[test]
fn consonants_last_the_same_at_any_rate() {
    let hz = 200.0;
    for sample_rate in [44100.0, 48000.0, 96000.0] {
        let samples = (0.2 * sample_rate) as usize;
        let start = (0.05 * sample_rate) as usize;
        let knobs = [1.0, 2.0];
        let plain = render::<SamSynth>(sample_rate, hz, knobs, samples, &[]);
        let triggered = render::<SamSynth>(sample_rate, hz, knobs, samples, &[start]);
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

/// A voice at `f0` cycles per 48 kHz sample, untriggered.
fn render_at<V: TestVoice>(f0: f32) -> Vec<f32> {
    let rates = Rates::new(48000.0);
    let mut ch = V::channel(&rates);
    (0..24000)
        .map(|_| ch.step(f0, knobs(1.5, 2.0), None, &rates).0)
        .collect()
}

/// Through-zero FM runs the glottis backward. SAM's sine formants restart
/// where a forward period ends, so its backward voice is its forward one
/// reversed; the filtered voices keep their spectrum.
#[test]
fn runs_backward() {
    let f0 = 1.0 / 256.0;
    let forward = render_at::<SamSynth>(f0);
    let backward = render_at::<SamSynth>(-f0);
    let peak = forward.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    // Past the limiter's settling, backward sample k retraces forward
    // sample n - k, n a whole number of periods.
    let n = 20478;
    let error = (12000..n)
        .map(|k| (backward[k] - forward[n - k]).abs())
        .fold(0.0f32, f32::max);
    assert!(peak > 0.1, "SAM: silent");
    assert!(error < 1e-3 * peak, "SAM: mirror error {error} of {peak}");

    fn same_spectrum<V: TestVoice>(f0: f32) {
        let hz = f0 * 48000.0;
        let forward = harmonics(&render_at::<V>(f0), hz, 48000.0, 24);
        let backward = harmonics(&render_at::<V>(-f0), hz, 48000.0, 24);
        let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
        for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
            assert!(
                (b - f).abs() < 0.05 * loudest,
                "{}, harmonic {}: {b} backward vs {f}",
                V::NAME,
                h + 1
            );
        }
    }
    same_spectrum::<NaiveSynth>(f0);
    same_spectrum::<LpcVowels>(f0);
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    fn check<V: TestVoice>() {
        let rates = Rates::new(48000.0);
        let mut held = V::channel(&rates);
        let mut glitched = V::channel(&rates);
        for n in 0..8000 {
            let v = match n % 2000 {
                500 => f32::NAN,
                501 => f32::INFINITY,
                502 => f32::NEG_INFINITY,
                _ => 1.0 + (n / 2000) as f32,
            };
            let steady = 1.0 + (n / 2000) as f32;
            let trigger = Some(if n % 1500 == 3 { 5.0 } else { 0.0 });
            let a = held.step(0.004, knobs(steady, steady), trigger, &rates);
            let b = glitched.step(0.004, knobs(v, v), trigger, &rates);
            assert_eq!(a, b, "{}, sample {n}", V::NAME);
        }
    }
    each_voice!(check);
}

/// Any knobs, pitches (NaN and infinite among them), triggers and knob jumps
/// leave both outputs finite at any engine rate. The limiter's slow release
/// lets a jump overshoot full scale a little, as in Plaits; the output stage
/// clips it.
#[test]
fn stays_finite() {
    fn check<V: TestVoice>() {
        for sample_rate in [8000.0, 22050.0, 44100.0, 96000.0, 192000.0] {
            let rates = Rates::new(sample_rate);
            let mut ch = V::channel(&rates);
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
                let knobs = knobs(jump + 2.5 * (t * 1.7).sin(), 2.5 + 3.0 * (t * 0.7).sin());
                let trigger = Some(if n % 701 == 0 { 5.0 } else { 0.0 });
                let (out, excitation) = ch.step(f0, knobs, trigger, &rates);
                assert!(
                    out.abs() < 1.5 && excitation.abs() < 3.0,
                    "{}, {sample_rate} Hz, sample {n}: {out}, {excitation}",
                    V::NAME
                );
            }
        }
    }
    each_voice!(check);
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
/// (5), its first formant low; `formantShift` raises the formants from 0 to
/// 5.
#[test]
fn vowel_and_formant_shift_move_the_formants_as_described() {
    fn check<V: TestVoice>() {
        let sample_rate = 48000.0;
        let hz = 100.0;
        let formant = |knobs: [f32; 2]| {
            first_formant(
                &outputs(&render::<V>(sample_rate, hz, knobs, 24000, &[])),
                hz,
                sample_rate,
            )
        };
        let (a, u) = (formant([0.0, 2.5]), formant([5.0, 2.5]));
        assert!(
            a > 1.25 * u,
            "{}: first formant {a} Hz at a, {u} Hz at u",
            V::NAME
        );
        let (deep, small) = (formant([0.0, 0.0]), formant([0.0, 5.0]));
        assert!(
            small > 1.15 * deep,
            "{}: first formant {deep} Hz at shift 0, {small} Hz at 5",
            V::NAME
        );
    }
    each_voice!(check);
}

/// Unpatching the trigger forgets its last state, so a trigger patched again
/// while high still rises.
#[test]
fn a_trigger_patched_again_rises() {
    let rates = Rates::new(48000.0);
    let mut ch = SamSynth::channel(&rates);
    for _ in 0..100 {
        ch.step(0.004, knobs(2.5, 2.5), Some(5.0), &rates);
    }
    ch.step(0.004, knobs(2.5, 2.5), None, &rates);
    assert!(ch.trigger.process(5.0));
}

/// A trigger changes each voice's sound for the syllable's start: a pitch
/// drop on the formant filters, a consonant on SAM and the LPC voice.
#[test]
fn a_trigger_starts_a_syllable() {
    fn check<V: TestVoice>() {
        let sample_rate = 48000.0;
        let start = 4800;
        let window = start..start + 2400;
        let knobs = [1.0, 2.5];
        let plain = outputs(&render::<V>(sample_rate, 130.0, knobs, 9600, &[]));
        let triggered = outputs(&render::<V>(sample_rate, 130.0, knobs, 9600, &[start]));
        let level: f32 = window.clone().map(|n| plain[n] * plain[n]).sum();
        let change: f32 = window
            .clone()
            .map(|n| (triggered[n] - plain[n]).powi(2))
            .sum();
        assert!(
            change > 0.1 * level,
            "{}: the trigger changes {change} of {level}",
            V::NAME
        );
        assert_eq!(plain[..start], triggered[..start], "{}", V::NAME);
    }
    each_voice!(check);
}
