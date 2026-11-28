use super::lane::{CombLane, Controls, Rates, damping_pole, even_ring_gain, pass_gain, ring_time};
use crate::dsp::utils::C4_HZ_F32;
use crate::types::GlideMark;

const SR: f32 = 48_000.0;

/// 220 Hz in V/Oct.
const A3: f32 = -0.25;

fn new_lane() -> (CombLane, Rates) {
    let rates = Rates::new(SR);
    let mut lane = CombLane::default();
    lane.allocate(&rates);
    (lane, rates)
}

fn render_ring(
    controls: Controls,
    even_ring: bool,
    input: impl Fn(usize) -> f32,
    len: usize,
) -> Vec<f32> {
    let (mut lane, rates) = new_lane();
    let marks = [GlideMark::ABSENT; 3];
    (0..len)
        .map(|n| lane.step(input(n), controls, marks, even_ring, &rates))
        .collect()
}

fn render(controls: Controls, input: impl Fn(usize) -> f32, len: usize) -> Vec<f32> {
    render_ring(controls, false, input, len)
}

fn impulse(n: usize) -> f32 {
    if n == 0 { 1.0 } else { 0.0 }
}

/// Magnitude of `y`'s component at `hz`.
fn magnitude_at(y: &[f32], hz: f32) -> f32 {
    let w = std::f32::consts::TAU * hz / SR;
    let (re, im) = y.iter().enumerate().fold((0.0, 0.0), |(re, im), (n, &v)| {
        let phase = w * n as f32;
        (re + v * phase.cos(), im - v * phase.sin())
    });
    (re * re + im * im).sqrt() / y.len() as f32
}

/// Frequency of the strongest response between `lo` and `hi` Hz, searched
/// in 0.1 Hz steps.
fn peak_between(y: &[f32], lo: f32, hi: f32) -> f32 {
    let steps = ((hi - lo) * 10.0) as usize;
    (0..=steps)
        .map(|i| lo + i as f32 * 0.1)
        .max_by(|&a, &b| magnitude_at(y, a).total_cmp(&magnitude_at(y, b)))
        .unwrap()
}

fn controls(freq: f32, feedback: f32, damp: f32) -> Controls {
    Controls {
        freq,
        feedback,
        damp,
    }
}

#[test]
fn feedback_zero_passes_the_input() {
    let y = render(controls(0.0, 0.0, 0.0), |n| (n as f32 * 0.01).sin(), 1000);
    for (n, v) in y.iter().enumerate() {
        assert_eq!(*v, (n as f32 * 0.01).sin());
    }
}

#[test]
fn rings_at_its_pitch_with_either_sign_and_any_damping() {
    let hz = 220.0;
    for feedback in [4.5, -4.5] {
        for damp in [0.0, 1.5, 3.0] {
            for even_ring in [false, true] {
                let y = render_ring(
                    controls(A3, feedback, damp),
                    even_ring,
                    impulse,
                    SR as usize,
                );
                let peak = peak_between(&y, hz * 0.9, hz * 1.1);
                let cents = 1200.0 * (peak / hz).log2();
                assert!(
                    cents.abs() < 3.0,
                    "feedback {feedback} damp {damp} even {even_ring}: peak {peak} Hz, {cents} cents"
                );
            }
        }
    }
}

#[test]
fn rings_in_tune_at_the_bottom_of_its_range() {
    // C1 ≈ 32.7 Hz, near the longest delay the line holds.
    let hz = C4_HZ_F32 / 8.0;
    for feedback in [4.5, -4.5] {
        let y = render(controls(-3.0, feedback, 1.0), impulse, 2 * SR as usize);
        let peak = peak_between(&y, hz * 0.95, hz * 1.05);
        let cents = 1200.0 * (peak / hz).log2();
        assert!(
            cents.abs() < 6.0,
            "feedback {feedback}: peak {peak} Hz, {cents} cents"
        );
    }
}

#[test]
fn negative_feedback_keeps_odd_harmonics() {
    let hz = 220.0;
    let y = render(controls(A3, -4.5, 0.0), impulse, SR as usize);
    let odd = magnitude_at(&y, 3.0 * hz);
    let even = magnitude_at(&y, 2.0 * hz);
    assert!(odd > 10.0 * even, "odd {odd} even {even}");
}

#[test]
fn damping_fades_high_harmonics_first() {
    let hz = 220.0;
    let bright = render(controls(A3, 4.5, 0.0), impulse, SR as usize);
    let dark = render(controls(A3, 4.5, 3.0), impulse, SR as usize);
    let ratio = |y: &[f32]| magnitude_at(y, 10.0 * hz) / magnitude_at(y, hz);
    assert!(ratio(&dark) < 0.5 * ratio(&bright));
}

#[test]
fn full_feedback_driven_hard_stays_bounded() {
    let y = render(
        controls(A3, 5.0, 0.0),
        |n| if (n / 109) % 2 == 0 { 5.0 } else { -5.0 },
        SR as usize,
    );
    assert!(y.iter().all(|v| v.is_finite() && v.abs() < 25.0));
}

#[test]
fn recovers_from_non_finite_input() {
    let (mut lane, rates) = new_lane();
    let marks = [GlideMark::ABSENT; 3];
    let c = controls(0.0, 5.0, 0.0);
    for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(lane.step(x, c, marks, true, &rates).is_finite());
    }
    let poisoned = controls(f32::NAN, f32::INFINITY, f32::NAN);
    for _ in 0..1000 {
        assert!(lane.step(1.0, poisoned, marks, true, &rates).is_finite());
    }
}

#[test]
fn shortest_and_longest_delays_stay_in_the_line() {
    for freq in [-20.0, -3.71, 8.0, 20.0, 200.0, 1e3, f32::MAX] {
        for feedback in [4.0, -4.0] {
            let y = render(controls(freq, feedback, 5.0), impulse, 20_000);
            assert!(y.iter().all(|v| v.is_finite()), "freq {freq}");
        }
    }
}

/// Decay in dB per second of `y`, from the RMS of two windows.
fn decay_db_per_second(y: &[f32]) -> f32 {
    let rms = |from: f32, to: f32| {
        let w = &y[(from * SR) as usize..(to * SR) as usize];
        (w.iter().map(|v| v * v).sum::<f32>() / w.len() as f32).sqrt()
    };
    20.0 * (rms(0.1, 0.2) / rms(0.6, 0.7)).log10() / 0.5
}

#[test]
fn every_pitch_rings_for_the_same_time() {
    for feedback in [3.0, -3.0] {
        for freq in [-2.0, A3, 2.0] {
            let y = render_ring(controls(freq, feedback, 0.0), true, impulse, SR as usize);
            let decay = decay_db_per_second(&y);
            let expected = 60.0 / ring_time(feedback);
            assert!(
                (decay / expected - 1.0).abs() < 0.1,
                "feedback {feedback} freq {freq}: {decay} dB/s, expected {expected}"
            );
        }
    }
}

#[test]
fn by_default_low_notes_ring_longer() {
    let low = render(controls(-2.0, 3.0, 0.0), impulse, SR as usize);
    let high = render(controls(1.0, 3.0, 0.0), impulse, SR as usize);
    assert!(decay_db_per_second(&high) > 4.0 * decay_db_per_second(&low));
}

#[test]
fn curves_span_their_ranges() {
    assert_eq!(pass_gain(0.0), 0.0);
    assert!((pass_gain(5.0) - 0.995).abs() < 1e-3);
    assert_eq!(pass_gain(-3.0), -pass_gain(3.0));
    assert_eq!(ring_time(0.0), 0.0);
    assert!((ring_time(3.0) - 1.0).abs() < 0.05);
    assert!((ring_time(-5.0) - 20.0).abs() < 1e-3);
    assert_eq!(even_ring_gain(0.0, 100.0, SR), 0.0);
    assert_eq!(
        even_ring_gain(-3.0, 100.0, SR),
        -even_ring_gain(3.0, 100.0, SR)
    );
    assert!(even_ring_gain(5.0, 2.0, SR) < 1.0);
    assert!(even_ring_gain(3.0, 50.0, SR) > even_ring_gain(3.0, 500.0, SR));
    assert_eq!(damping_pole(0.0, SR), 0.0);
    let mut last = 0.0;
    for d in 1..=5 {
        let pole = damping_pole(d as f32, SR);
        assert!(pole > last && pole < 1.0);
        last = pole;
    }
}
