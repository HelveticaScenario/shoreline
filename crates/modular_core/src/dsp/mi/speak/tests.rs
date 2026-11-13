use super::*;
use crate::dsp::mi::core::fixture::{assert_rate_independent, harmonics, max_error, records};
use crate::dsp::mi::core::random::Random;
use crate::dsp::mi::core::tables::{LPC_BANK_WORDS, LPC_WORD_FRAMES, LPC_WORD_STARTS};
use crate::poly::PolySignal;
use crate::types::Signal;

fn mark() -> GlideMark {
    PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
}

/// Knobs in volts: bank, then word, formant shift, prosody, speed and accent.
fn knobs(bank: f32, [word, shift, prosody, speed, accent]: [f32; 5]) -> Knobs {
    Knobs {
        bank,
        smooth: [word, shift, prosody, speed, accent].map(|v| (v, mark())),
    }
}

/// A lane as `speak.cc` starts it, its noise seeded as the driver seeds
/// stmlib's generator.
fn new_lane() -> Lane {
    let mut lane = Lane::default();
    lane.speaker.random = Random::new(0x21);
    lane.speaker.seeded = true;
    lane
}

/// Replays `speak.f32`: Plaits' controller on its word banks, then its
/// limiter, through scans, triggered words, sweeps of every knob, bank
/// switches, a high pitch and a trigger unpatched mid-word.
#[test]
fn matches_plaits() {
    let rates = Rates::new(48000.0);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for records in records("speak", 8 + 2).chunks(24000) {
        let mut lane = new_lane();
        for r in records {
            let trigger = (r[7] >= 0.0).then_some(r[7]);
            let (out, excitation) = lane.step(
                r[0],
                knobs(r[1], [r[2], r[3], r[4], r[5], r[6]]),
                trigger,
                &rates,
            );
            actual.extend([out, excitation]);
            expected.extend([r[8], r[9]]);
        }
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// Default knobs but `bank`, `word` and those given.
const DEFAULTS: [f32; 5] = [0.0, 2.5, 0.0, 2.5, 4.0];

/// `samples` of a voice at `hz`: the output and the excitation, as pairs. A
/// trigger, patched when `triggers` is, rises on each sample in it.
fn render(
    sample_rate: f32,
    hz: f32,
    bank: f32,
    smooth: [f32; 5],
    samples: usize,
    triggers: Option<&[usize]>,
) -> Vec<(f32, f32)> {
    let rates = Rates::new(sample_rate);
    let mut lane = new_lane();
    (0..samples)
        .map(|n| {
            let trigger = triggers.map(|t| if t.contains(&n) { 5.0 } else { 0.0 });
            lane.step(hz / sample_rate, knobs(bank, smooth), trigger, &rates)
        })
        .collect()
}

fn outputs(x: &[(f32, f32)]) -> Vec<f32> {
    x.iter().map(|&(out, _)| out).collect()
}

fn excitations(x: &[(f32, f32)]) -> Vec<f32> {
    x.iter().map(|&(_, e)| e).collect()
}

/// The number of words in `bank`.
fn words(bank: usize) -> usize {
    usize::from(LPC_BANK_WORDS[bank + 1] - LPC_BANK_WORDS[bank])
}

/// A scan held mid-word has the same harmonics at 44.1 and 96 kHz as
/// at 48 kHz, in the voice and in its excitation.
#[test]
fn sounds_the_same_at_any_rate() {
    let hz = 110.0;
    for (bank, word, shift) in [(0.0, 3.0, 2.5), (1.0, 4.0, 4.0), (4.0, 11.0, 1.0)] {
        let smooth = [word, shift, 0.0, 2.5, 4.0];
        let render = |sample_rate: f32| {
            render(
                sample_rate,
                hz,
                bank,
                smooth,
                (0.5 * sample_rate) as usize,
                None,
            )
        };
        let level = harmonics(&outputs(&render(48000.0)), hz, 48000.0, 16)
            .iter()
            .fold(0.0f32, |m, &a| m.max(a));
        assert!(level > 0.05, "bank {bank} word {word}: silent ({level})");
        assert_rate_independent(|sample_rate| outputs(&render(sample_rate)), hz, 0.03);
        assert_rate_independent(|sample_rate| excitations(&render(sample_rate)), hz, 0.03);
    }
}

/// Seconds from a word's trigger to the end of its excitation, listening for
/// `longest` seconds.
fn spoken_seconds(sample_rate: f32, bank: f32, word: f32, speed: f32, longest: f32) -> f32 {
    let start = (0.05 * sample_rate) as usize;
    let samples = start + (longest * sample_rate) as usize;
    let smooth = [word, 2.5, 0.0, speed, 4.0];
    let excitation = excitations(&render(
        sample_rate,
        120.0,
        bank,
        smooth,
        samples,
        Some(&[start]),
    ));
    let last = (start..samples)
        .rev()
        .find(|&n| excitation[n] != 0.0)
        .unwrap();
    (last - start) as f32 / sample_rate
}

/// Whether a word that took `seconds` lasted `expected`, well within a
/// 25 ms frame: the glottal pulses can end up to a period of the 120 Hz voice
/// before the frame does.
fn said_within_a_frame(seconds: f32, expected: f32) -> bool {
    (expected - 1.0 / 120.0 - 0.001..expected + 0.002).contains(&seconds)
}

/// The duration of a bank's word up to its last sounding frame, at 40 frames
/// a second.
fn recorded_seconds(bank: usize, index: usize) -> f32 {
    let word = usize::from(LPC_BANK_WORDS[bank]) + index;
    let start = usize::from(LPC_WORD_STARTS[word]);
    let end = usize::from(LPC_WORD_STARTS[word + 1]);
    let last = (start..end)
        .rev()
        .find(|&f| LPC_WORD_FRAMES[f * 12] != 0)
        .unwrap();
    (last + 1 - start) as f32 / 40.0
}

/// `bank` and `word` pick the word a trigger says, which lasts as recorded at
/// any rate, then falls silent.
#[test]
fn a_triggered_word_lasts_as_recorded_at_any_rate() {
    for (bank, index) in [(0, 0), (1, 8), (2, 25), (3, 5), (4, 18)] {
        let expected = recorded_seconds(bank, index);
        for sample_rate in [44100.0, 48000.0, 96000.0] {
            let seconds =
                spoken_seconds(sample_rate, bank as f32, index as f32, 2.5, expected + 0.2);
            assert!(
                said_within_a_frame(seconds, expected),
                "bank {bank} word {index} at {sample_rate} Hz: {seconds} s, recorded {expected} s"
            );
        }
    }
}

/// `speed` 5 says a word four times faster than recorded, 0 four times
/// slower.
#[test]
fn speed_sets_how_fast_a_word_is_said() {
    let recorded = recorded_seconds(4, 18);
    let word = 18.0;
    let fast = spoken_seconds(48000.0, 4.0, word, 5.0, recorded);
    let slow = spoken_seconds(48000.0, 4.0, word, 0.0, recorded * 4.0 + 0.5);
    assert!((fast - recorded / 4.0).abs() < 0.03, "{fast} s fast");
    assert!((slow - recorded * 4.0).abs() < 0.1, "{slow} s slow");
}

/// `accent` scales a triggered word; with the trigger unpatched it does
/// nothing.
#[test]
fn accent_scales_a_triggered_word() {
    let excitation = |accent: f32, triggers: Option<&[usize]>| {
        excitations(&render(
            48000.0,
            120.0,
            1.0,
            [1.0, 2.5, 0.0, 2.5, accent],
            24000,
            triggers,
        ))
    };
    let loud = excitation(5.0, Some(&[100]));
    let soft = excitation(2.5, Some(&[100]));
    assert!(loud.iter().any(|&e| e.abs() > 0.1));
    for (l, s) in loud.iter().zip(&soft) {
        assert!((0.5 * l - s).abs() < 1e-6, "{s} vs half of {l}");
    }
    assert_eq!(excitation(5.0, None), excitation(1.0, None));
}

/// The period of `x`'s strongest repetition between `min` and `max` samples.
fn period(x: &[f32], min: usize, max: usize) -> usize {
    let x = &x[x.len() / 2..];
    (min..max)
        .max_by(|&a, &b| {
            let r = |lag: usize| -> f32 { x.iter().zip(&x[lag..]).map(|(p, q)| p * q).sum() };
            r(a).total_cmp(&r(b))
        })
        .unwrap()
}

/// `prosody` 0 sings at the pitch; 5 sings a frame at the pitch the word was
/// recorded at, relative to its 100 Hz norm.
#[test]
fn prosody_follows_the_recorded_pitch() {
    // Two voiced frames of the synth words, both pitch period 49 at 8 kHz:
    // 163 Hz for a voice normalised to 100 Hz.
    let bank = 4;
    let frames = usize::from(LPC_WORD_STARTS[usize::from(LPC_BANK_WORDS[bank + 1])])
        - usize::from(LPC_WORD_STARTS[usize::from(LPC_BANK_WORDS[bank])]);
    let first = usize::from(LPC_WORD_STARTS[usize::from(LPC_BANK_WORDS[bank])]);
    let at = (first..first + frames - 1)
        .find(|&f| LPC_WORD_FRAMES[f * 12 + 1] == 49 && LPC_WORD_FRAMES[(f + 1) * 12 + 1] == 49)
        .unwrap();
    // The word position that scans halfway between them.
    let first_word = usize::from(LPC_BANK_WORDS[bank]);
    let index = (0..words(bank))
        .rfind(|&w| usize::from(LPC_WORD_STARTS[first_word + w]) <= at)
        .unwrap();
    let start = usize::from(LPC_WORD_STARTS[first_word + index]);
    let length = usize::from(LPC_WORD_STARTS[first_word + index + 1]) - start;
    let word = index as f32 - 0.5 + (at - start) as f32 / length as f32 + 0.5 / length as f32;
    let pitch = |prosody: f32, lags: (usize, usize)| {
        let x = render(
            48000.0,
            100.0,
            bank as f32,
            [word, 2.5, prosody, 2.5, 4.0],
            48000,
            None,
        );
        period(&excitations(&x), lags.0, lags.1)
    };
    assert_eq!(pitch(0.0, (360, 720)), 480);
    let recorded = pitch(5.0, (200, 400));
    assert!((recorded as f32 - 294.0).abs() < 2.0, "{recorded}");
}

/// A voice at `f0` cycles per 48 kHz sample, scanning.
fn render_at(f0: f32) -> Vec<f32> {
    let rates = Rates::new(48000.0);
    let mut lane = new_lane();
    (0..24000)
        .map(|_| {
            lane.step(f0, knobs(1.0, [4.0, 2.0, 0.0, 2.5, 4.0]), None, &rates)
                .0
        })
        .collect()
}

/// Through-zero FM runs the glottis backward, keeping the voice's spectrum.
#[test]
fn runs_backward() {
    let f0 = 1.0 / 256.0;
    let hz = f0 * 48000.0;
    let forward = harmonics(&render_at(f0), hz, 48000.0, 24);
    let backward = harmonics(&render_at(-f0), hz, 48000.0, 24);
    let loudest = forward.iter().fold(0.0f32, |m, &a| m.max(a));
    assert!(loudest > 0.05, "silent");
    for (h, (b, f)) in backward.iter().zip(&forward).enumerate() {
        assert!(
            (b - f).abs() < 0.05 * loudest,
            "harmonic {}: {b} backward vs {f}",
            h + 1
        );
    }
}

/// Every whole `word` of every bank says its own word, lasting as recorded.
#[test]
fn each_word_index_says_its_word() {
    for bank in 0..BANKS {
        for index in 0..words(bank) {
            let expected = recorded_seconds(bank, index);
            let seconds = spoken_seconds(48000.0, bank as f32, index as f32, 2.5, expected + 0.2);
            assert!(
                said_within_a_frame(seconds, expected),
                "bank {bank} word {index}: {seconds} s, recorded {expected} s"
            );
        }
        // Past the last index, the last word.
        let last = words(bank) - 1;
        let expected = recorded_seconds(bank, last);
        let seconds = spoken_seconds(48000.0, bank as f32, 40.0, 2.5, expected + 0.2);
        assert!(
            said_within_a_frame(seconds, expected),
            "bank {bank} past the end: {seconds} s"
        );
    }
}

/// With the trigger patched, the voice is silent until the first trigger and
/// again once the bank changes, after the frame playing; unpatched, it scans
/// the bank throughout.
#[test]
fn a_patched_trigger_is_silent_between_words() {
    let rates = Rates::new(48000.0);
    let render = |trigger: bool| {
        let mut lane = new_lane();
        (0..48000)
            .map(|n| {
                let bank = if n < 24000 { 1.0 } else { 3.0 };
                let pulse = if n == 12000 { 5.0 } else { 0.0 };
                let knobs = knobs(bank, [4.0, 2.5, 0.0, 2.5, 4.0]);
                lane.step(120.0 / 48000.0, knobs, trigger.then_some(pulse), &rates)
                    .0
            })
            .collect::<Vec<_>>()
    };
    let patched = render(true);
    let scanned = render(false);
    let loudest = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert_eq!(loudest(&patched[..12000]), 0.0, "before the first trigger");
    assert!(loudest(&patched[12000..24000]) > 0.05, "the word");
    // The word's frame playing at the change runs its 25 ms out first.
    assert_eq!(
        loudest(&patched[24000 + 1300..]),
        0.0,
        "after the bank change"
    );
    assert!(loudest(&scanned[..12000]) > 0.05 && loudest(&scanned[24000 + 1300..]) > 0.05);
}

/// A NaN or infinite knob holds the sound where it is.
#[test]
fn non_finite_knobs_hold() {
    let rates = Rates::new(48000.0);
    let mut held = new_lane();
    let mut glitched = new_lane();
    for n in 0..20000 {
        let steady = 0.5 + (n / 5000) as f32;
        let v = match n % 5000 {
            500 => f32::NAN,
            501 => f32::INFINITY,
            502 => f32::NEG_INFINITY,
            _ => steady,
        };
        let trigger = Some(if n % 3000 == 3 { 5.0 } else { 0.0 });
        let bank = (n / 5000) as f32;
        let a = held.step(0.004, knobs(bank, [steady; 5]), trigger, &rates);
        let glitched_bank = if v.is_nan() { v } else { bank };
        let b = glitched.step(0.004, knobs(glitched_bank, [v; 5]), trigger, &rates);
        assert_eq!(a, b, "sample {n}");
    }
}

/// Any knobs, banks, pitches (NaN and infinite among them), triggers and
/// knob jumps leave both outputs finite at any engine rate.
#[test]
fn stays_finite() {
    for sample_rate in [8000.0, 22050.0, 44100.0, 96000.0, 192000.0] {
        let rates = Rates::new(sample_rate);
        let mut lane = new_lane();
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
            let smooth = [
                jump + 2.5 * (t * 1.7).sin(),
                2.5 + 3.0 * (t * 0.7).sin(),
                2.5 + 3.0 * (t * 0.3).sin(),
                2.5 + 3.0 * (t * 1.3).sin() - jump,
                2.5 + 3.0 * (t * 0.9).sin(),
            ];
            let bank = 2.0 + 3.0 * (t * 0.5).sin();
            let trigger = match n % 7000 {
                0..=3000 => Some(if n % 701 == 0 { 5.0 } else { 0.0 }),
                _ => None,
            };
            let (out, excitation) = lane.output(f0, knobs(bank, smooth), trigger, &rates);
            assert!(
                out.is_finite() && excitation.is_finite(),
                "{sample_rate} Hz, sample {n}: {out}, {excitation}"
            );
        }
    }
}

/// Unpatching the trigger forgets its last state, so a trigger patched again
/// while high still rises.
#[test]
fn a_trigger_patched_again_rises() {
    let rates = Rates::new(48000.0);
    let mut lane = new_lane();
    for _ in 0..100 {
        lane.step(0.004, knobs(0.0, DEFAULTS), Some(5.0), &rates);
    }
    lane.step(0.004, knobs(0.0, DEFAULTS), None, &rates);
    assert!(lane.trigger.process(5.0));
}
