use super::ensemble::{Ensemble, EnsembleRates};
use super::{fold_to_stereo, hold_knobs};
use crate::dsp::mi::core::fixture::records;

/// Replays `chorus.f32` (Plaits' ensemble, a sample at a time) on a stereo
/// tone with the amount and depth sweeping: bit for bit, since no step goes
/// through a maths library.
#[test]
fn matches_plaits() {
    let rates = EnsembleRates::new(48000.0);
    let mut ensemble = Ensemble::new(&rates);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for r in records("chorus", 4 + 2) {
        let (left, right) = ensemble.process(r[0], r[1], r[2], r[3], &rates);
        actual.extend([left, right]);
        expected.extend([r[4], r[5]]);
    }
    for (n, (a, e)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(a.to_bits(), e.to_bits(), "sample {}: {a} vs {e}", n / 2);
    }
}

/// With the sweep at rest the taps sit 4 ms behind the input at any rate: a
/// click returns that long after, on both sides.
#[test]
fn taps_sit_at_the_same_delay_at_any_rate() {
    for sample_rate in [8000.0f32, 44100.0, 48000.0, 96000.0, 192000.0] {
        let rates = EnsembleRates::new(sample_rate);
        let mut ensemble = Ensemble::new(&rates);
        let response: Vec<(f32, f32)> = (0..(sample_rate * 0.01) as usize)
            .map(|n| {
                let x = if n == 0 { 1.0 } else { 0.0 };
                ensemble.process(x, x, 1.0, 0.0, &rates)
            })
            .collect();
        let peak = |side: fn(&(f32, f32)) -> f32| {
            let (at, _) = response
                .iter()
                .enumerate()
                .skip(1)
                .fold((0, 0.0f32), |best, (n, s)| {
                    if side(s).abs() > best.1 {
                        (n, side(s).abs())
                    } else {
                        best
                    }
                });
            at as f32 / sample_rate * 1000.0
        };
        for (name, ms) in [("left", peak(|s| s.0)), ("right", peak(|s| s.1))] {
            assert!((ms - 4.0).abs() < 0.05, "{sample_rate} Hz {name}: {ms} ms");
        }
    }
}

/// The swept chorus of a tone is as loud at 44.1 and 96 kHz as at 48 kHz.
#[test]
fn chorus_is_as_loud_at_any_rate() {
    let level = |sample_rate: f32| {
        let rates = EnsembleRates::new(sample_rate);
        let mut ensemble = Ensemble::new(&rates);
        let n = (sample_rate * 4.0) as usize;
        let mut sum = 0.0f64;
        for i in 0..n {
            let t = i as f32 / sample_rate;
            let x = (std::f32::consts::TAU * 440.0 * t).sin();
            let (l, r) = ensemble.process(x, -x, 1.0, 1.0, &rates);
            sum += f64::from(l * l + r * r);
        }
        (sum / n as f64).sqrt() as f32
    };
    let reference = level(48000.0);
    for sample_rate in [8000.0, 44100.0, 96000.0, 192000.0] {
        let db = 20.0 * (level(sample_rate) / reference).log10();
        assert!(db.abs() < 0.2, "{sample_rate} Hz: {db} dB");
    }
}

/// A mono signal on both sides comes out the same on both sides at no sweep,
/// and dry at no amount.
#[test]
fn amount_and_depth_reach_their_ends() {
    let rates = EnsembleRates::new(48000.0);
    let mut dry = Ensemble::new(&rates);
    for n in 0..4800 {
        let x = (n as f32 * 0.05).sin();
        let (l, r) = dry.process(x, x, 0.0, 0.7, &rates);
        assert_eq!((l, r), (x, x), "sample {n}");
    }
    let mut still = Ensemble::new(&rates);
    for n in 0..4800 {
        let x = (n as f32 * 0.05).sin();
        let (l, r) = still.process(x, x, 0.8, 0.0, &rates);
        assert_eq!(l, r, "sample {n}");
    }
}

/// The sides stay apart: a click on one side reaches the other only through
/// the crossed third tap, at a third of the level, and returns at the same
/// delay at any rate.
#[test]
fn sides_cross_through_one_tap() {
    for sample_rate in [8000.0f32, 48000.0, 192000.0] {
        let rates = EnsembleRates::new(sample_rate);
        let mut ensemble = Ensemble::new(&rates);
        let (mut left_peak, mut right_peak) = (0.0f32, 0.0f32);
        for n in 1..(sample_rate * 0.01) as usize {
            let x = if n == 1 { 1.0 } else { 0.0 };
            let (l, r) = ensemble.process(x, 0.0, 1.0, 0.0, &rates);
            if n > 1 {
                left_peak = left_peak.max(l.abs());
                right_peak = right_peak.max(r.abs());
            }
        }
        assert!(
            (left_peak - 0.66).abs() < 1e-3,
            "{sample_rate} Hz: left {left_peak}"
        );
        assert!(
            (right_peak - 0.33).abs() < 1e-3,
            "{sample_rate} Hz: right {right_peak}"
        );
    }
}

/// A poly input folds to stereo: even channels left, odd right, an odd last
/// channel to both, a non-finite sample as 0.
#[test]
fn input_folds_to_stereo() {
    let values = [1.0, 10.0, 100.0, f32::NAN, 1000.0];
    let fold = |channels| fold_to_stereo(channels, |ch| values[ch]);
    assert_eq!(fold(0), (0.0, 0.0));
    assert_eq!(fold(1), (1.0, 1.0));
    assert_eq!(fold(2), (1.0, 10.0));
    assert_eq!(fold(3), (101.0, 110.0));
    assert_eq!(fold(4), (101.0, 10.0));
    assert_eq!(fold(5), (1101.0, 1010.0));
}

/// The knobs map 0–5 V to 0–1, clamped, and a NaN or infinite knob holds the
/// last finite one.
#[test]
fn knobs_map_and_hold() {
    let mut held = [2.5, 2.5];
    assert_eq!(hold_knobs(&mut held, [5.0, 0.0]), [1.0, 0.0]);
    assert_eq!(hold_knobs(&mut held, [2.5, 7.0]), [0.5, 1.0]);
    assert_eq!(hold_knobs(&mut held, [f32::NAN, f32::INFINITY]), [0.5, 1.0]);
    assert_eq!(hold_knobs(&mut held, [-1.0, 1.25]), [0.0, 0.25]);
}
