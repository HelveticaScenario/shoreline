use super::{HATS, METAL, SR, centroid, ring_hit, square_hit};
use crate::dsp::mi::drum::measure::{level, peak, rms, window};

/// A hat struck at 48 kHz at 300 Hz with knob `index` at `volts`, the rest
/// at `knobs`, with noise from `seed`.
fn with(render: super::Render, knobs: [f32; 4], index: usize, volts: f32, seed: u32) -> Vec<f32> {
    let mut knobs = knobs;
    knobs[index] = volts;
    render(SR, 300.0, knobs, seed, 0.5)
}

/// How fast `x` fades: its level from 0.2 to 0.4 s against its first 0.1 s.
fn tail(x: &[f32]) -> f32 {
    level(x, SR, 0.2, 0.4) - level(x, SR, 0.0, 0.1)
}

const TONE: usize = 0;
const DECAY: usize = 1;
const NOISINESS: usize = 2;
const ACCENT: usize = 3;

/// Tone raises the band the metal sounds in.
#[test]
fn tone_brightens() {
    for (name, render) in HATS {
        let centre = |volts| {
            let x = with(render, METAL, TONE, volts, 1);
            centroid(window(&x, SR, 0.0, 0.1), SR)
        };
        let (dull, mid, bright) = (centre(0.0), centre(2.5), centre(5.0));
        assert!(
            dull * 1.1 < mid && mid * 1.5 < bright,
            "{name}: centred at {dull} {mid} {bright} Hz"
        );
    }
}

/// Decay lengthens the ring.
#[test]
fn decay_lengthens() {
    for (name, render) in HATS {
        let (short, long) = (
            with(render, METAL, DECAY, 0.0, 1),
            with(render, METAL, DECAY, 5.0, 1),
        );
        assert!(tail(&long) > tail(&short) + 20.0, "{name}");
    }
}

/// Noisiness blends in noise, slowly at first: the part of the sound that
/// differs between two noise seeds grows a quarter as fast at 2.5 V as at
/// 5 V, and none at 0.
#[test]
fn noisiness_blends_in_noise() {
    for (name, render) in HATS {
        let spread = |volts| {
            let a = with(render, METAL, NOISINESS, volts, 1);
            let b = with(render, METAL, NOISINESS, volts, 2);
            let d: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x - y).collect();
            rms(window(&d, SR, 0.0, 0.1))
        };
        assert_eq!(spread(0.0), 0.0, "{name}: no noise at 0");
        let (half, full) = (spread(2.5), spread(5.0));
        assert!(
            half < 0.4 * full && half > 0.1 * full,
            "{name}: {half} vs {full}"
        );
    }
}

/// Accent makes each hit louder; on the ring hat it also delays the choke, so
/// a harder hit's tail fades more slowly against its start.
#[test]
fn accent_louder_and_rings_hat_longer() {
    let knobs = [2.5, 4.0, 0.0, 4.0];
    for (name, render) in HATS {
        let (soft, hard) = (
            with(render, knobs, ACCENT, 1.0, 1),
            with(render, knobs, ACCENT, 5.0, 1),
        );
        assert!(peak(&hard) > 1.5 * peak(&soft), "{name}: louder");
    }
    let (soft, hard) = (
        with(square_hit, knobs, ACCENT, 1.0, 1),
        with(square_hit, knobs, ACCENT, 5.0, 1),
    );
    assert!((tail(&hard) - tail(&soft)).abs() < 0.5, "hiHat: same fade");
    let (soft, hard) = (
        with(ring_hit, knobs, ACCENT, 1.0, 1),
        with(ring_hit, knobs, ACCENT, 5.0, 1),
    );
    assert!(tail(&hard) > tail(&soft) + 6.0, "ringHat: chokes later");
}
