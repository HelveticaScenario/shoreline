use super::ensemble::{Ensemble, EnsembleRates};
use crate::dsp::mi::core::fixture::{max_error, records};

/// Replays `chorus.f32` (Plaits' ensemble, a sample at a time) on a stereo
/// tone with the amount and depth sweeping.
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
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

/// With the sweep at rest the taps sit 4 ms behind the input at any rate: a
/// click returns that long after, on both sides.
#[test]
fn taps_sit_at_the_same_delay_at_any_rate() {
    for sample_rate in [44100.0f32, 48000.0, 96000.0] {
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
    for sample_rate in [44100.0, 96000.0] {
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
