use super::*;
use crate::dsp::mi::core::fixture::harmonics;

/// A tiny deterministic generator for the fuzz tests.
struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// The share of a tone's power, in dB, that falls between the harmonics of
/// `hz`: aliasing and steps left un-band-limited. The tone's second half must
/// hold whole cycles.
fn inharmonic_db(x: &[f32], hz: f32, sample_rate: f32) -> f32 {
    let half = &x[x.len() / 2..];
    let mean = half.iter().map(|&v| f64::from(v)).sum::<f64>() / half.len() as f64;
    let total = half
        .iter()
        .map(|&v| (f64::from(v) - mean).powi(2))
        .sum::<f64>()
        / half.len() as f64;
    let count = ((0.5 * sample_rate / hz) as usize).saturating_sub(1);
    let harmonic: f64 = harmonics(x, hz, sample_rate, count)
        .iter()
        .map(|&a| 0.5 * f64::from(a).powi(2))
        .sum();
    (10.0 * ((total - harmonic).max(1e-12) / total).log10()) as f32
}

/// Running backward, a synced slave plays the forward waveform reversed, so it
/// is as clean as forward at whole-number ratios, where the restart meets the
/// end of a slave cycle, and either side of the width.
#[test]
fn backward_sync_is_band_limited() {
    let sample_rate = 48000.0;
    let hz = 400.0;
    for ratio in [1.0, 2.0, 3.0, 1.5, 2.5, 1.003, 2.005] {
        for (pw, waveshape) in [(0.5, 1.0), (0.5, 0.5), (0.995, 1.0), (0.7, 0.0)] {
            let render = |direction: f32| {
                let mut oscillator = VariableShapeOscillator::default();
                let master = direction * hz / sample_rate;
                (0..48000)
                    .map(|_| oscillator.step_synced(master, master * ratio, pw, waveshape, 0.25))
                    .collect::<Vec<f32>>()
            };
            let forward = render(1.0);
            let backward = render(-1.0);
            let peak = |x: &[f32]| x[4800..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(
                peak(&backward) < 1.05 * peak(&forward),
                "ratio {ratio}, pw {pw}, shape {waveshape}: peak {} vs {}",
                peak(&backward),
                peak(&forward)
            );
            let (f, b) = (
                inharmonic_db(&forward, hz, sample_rate),
                inharmonic_db(&backward, hz, sample_rate),
            );
            // Below −60 dB both are clean, and the floor is rounding.
            assert!(
                b < (f + 3.0).max(-60.0),
                "ratio {ratio}, pw {pw}, shape {waveshape}: {b} dB backward vs {f} dB forward"
            );
        }
    }
}

/// Random pitches of either sign up to the limit an 8 kHz rate allows, random
/// widths, shapes and reversals keep both phases within a cycle and the output
/// near its swing; past a quarter cycle per sample several edges share each
/// sample, and their steps add up.
#[test]
fn stays_within_a_cycle() {
    for (max, swing) in [(0.25, 2.0), (1.09, 6.0), (1.5, 6.0)] {
        let mut random = Lcg(7);
        let mut oscillator = VariableShapeOscillator::default();
        let (mut master, mut ratio, mut pw, mut waveshape) = (0.01, 2.0, 0.5, 0.5);
        for n in 0..200_000 {
            if n % 37 == 0 {
                let sign = if random.next() < 0.5 { -1.0 } else { 1.0 };
                master = sign * max * random.next() * random.next();
                ratio = 1.0 + 15.0 * random.next();
                pw = random.next();
                waveshape = random.next();
            }
            let out = oscillator.step_synced(master, master * ratio, pw, waveshape, max);
            assert!(
                (0.0..1.0).contains(&oscillator.phase)
                    && (0.0..1.0).contains(&oscillator.master_phase),
                "max {max}, sample {n}: phases {} {}",
                oscillator.phase,
                oscillator.master_phase
            );
            assert!(
                out.is_finite() && out.abs() < swing,
                "max {max}, sample {n}: {out}"
            );
        }
    }
}
