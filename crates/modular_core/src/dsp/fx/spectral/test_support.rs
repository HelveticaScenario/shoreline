//! Signals, a driver and spectrum measures shared by the spectral reverbs'
//! tests.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

use crate::dsp::utils::rng::{LcgRng, TEST_SEED_BASE};
use crate::poly::{LaneSum, PairSignal, PolySignal};
use crate::types::Signal;

pub const SAMPLE_RATE: f32 = 48_000.0;
/// Length of [`burst`] and [`noise_burst`] in samples.
pub const BURST: usize = 12_000;

/// A module the tests feed one sample at a time.
pub trait Driven {
    fn set_input(&mut self, input: PairSignal);
    fn set_freeze(&mut self, freeze: Option<LaneSum>);
    fn step(&mut self) -> [f32; 2];
}

pub fn control(volts: f32) -> Option<LaneSum> {
    Some(LaneSum::from_poly(PolySignal::mono(Signal::Volts(volts))))
}

/// Pin every module's random streams for the rest of the test.
pub fn pin_seed() {
    TEST_SEED_BASE.set(Some(0x5EED));
}

/// A 1 V sine at 440 Hz.
pub fn tone(n: usize) -> f32 {
    (std::f32::consts::TAU * 440.0 * n as f32 / SAMPLE_RATE).sin()
}

/// [`tone`] for the first [`BURST`] samples, then silence.
pub fn burst(n: usize) -> f32 {
    if n < BURST { tone(n) } else { 0.0 }
}

/// White noise for the first [`BURST`] samples, then silence.
pub fn noise_burst() -> impl FnMut(usize) -> f32 {
    let mut rng = LcgRng::default();
    rng.seed(7, 0);
    move |n| if n < BURST { rng.next_bipolar() } else { 0.0 }
}

/// `frames` samples of each side of `module` fed `input` on both sides, with
/// `freeze` high from sample `freeze_from` on.
pub fn run(
    module: &mut impl Driven,
    frames: usize,
    mut input: impl FnMut(usize) -> f32,
    freeze_from: Option<usize>,
) -> (Vec<f32>, Vec<f32>) {
    let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    for n in 0..frames {
        module.set_input(PairSignal::from_mono(PolySignal::mono(Signal::Volts(
            input(n),
        ))));
        if freeze_from == Some(n) {
            module.set_freeze(control(5.0));
        }
        let [l, r] = module.step();
        left.push(l);
        right.push(r);
    }
    (left, right)
}

pub fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| s.abs().max(m))
}

/// `(frequency, power)` of every bin of the Hann-windowed `samples`.
fn spectrum(samples: &[f32]) -> Vec<(f32, f32)> {
    let size = samples.len();
    let mut bins: Vec<Complex<f32>> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos();
            Complex::new(s * w, 0.0)
        })
        .collect();
    FftPlanner::new().plan_fft_forward(size).process(&mut bins);
    bins[..size / 2]
        .iter()
        .enumerate()
        .map(|(k, bin)| (k as f32 * SAMPLE_RATE / size as f32, bin.norm_sqr()))
        .collect()
}

/// Power of `samples` in the 100–400 Hz and 4–12 kHz bands.
pub fn band_powers(samples: &[f32]) -> (f32, f32) {
    let spectrum = spectrum(samples);
    let band = |from: f32, to: f32| -> f32 {
        spectrum
            .iter()
            .filter(|(f, _)| (from..to).contains(f))
            .map(|(_, p)| p)
            .sum()
    };
    (band(100.0, 400.0), band(4_000.0, 12_000.0))
}

/// Power-weighted mean frequency of `samples`, and the standard deviation
/// around it, in Hz.
pub fn centroid_and_spread(samples: &[f32]) -> (f32, f32) {
    let spectrum = spectrum(samples);
    let total: f32 = spectrum.iter().map(|(_, p)| p).sum();
    let centroid = spectrum.iter().map(|(f, p)| f * p).sum::<f32>() / total;
    let variance = spectrum
        .iter()
        .map(|(f, p)| (f - centroid).powi(2) * p)
        .sum::<f32>()
        / total;
    (centroid, variance.sqrt())
}
