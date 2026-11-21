//! `$wavetable` plays a bright table alias-free at every pitch: the spectrum it
//! renders holds energy only at harmonics of the note, and a note just below
//! Nyquist still sounds its fundamental.

mod block_common;

use std::sync::Arc;

use block_common::{BLOCK_SIZE, SAMPLE_RATE, assemble};
use modular_core::types::{ProcessingMode, SampleBuffer, WavData};
use rustfft::{FftPlanner, num_complex::Complex};
use serde_json::json;

const FRAME: usize = 2048;
const FFT_SIZE: usize = 1 << 15;
/// Samples skipped before analysis, past the oscillator's start.
const SETTLE: usize = 1024;

/// One frame of a saw holding every harmonic the frame can store.
fn saw_table() -> Arc<WavData> {
    let samples = (0..FRAME)
        .map(|i| {
            let phase = std::f32::consts::TAU * i as f32 / FRAME as f32;
            (1..FRAME / 2)
                .map(|k| (phase * k as f32).sin() / k as f32)
                .sum::<f32>()
                * 0.55
        })
        .collect();
    Arc::new(WavData::new(
        SampleBuffer::from_samples(vec![samples], SAMPLE_RATE),
        Some(FRAME),
    ))
}

/// `len` samples of `$wavetable` playing the saw table at `hz`.
fn render(hz: f32, len: usize) -> Vec<f32> {
    let pitch = (hz / 261.625_58).log2();
    let params = json!({
        "wav": { "type": "wav_ref", "path": "saw", "channels": 1 },
        "pitch": pitch,
    });
    let patch = assemble(
        [("osc", "$wavetable", params, ProcessingMode::Block)],
        [("saw", saw_table())],
    );
    let osc = &patch.sampleables["osc"];
    let mut out = Vec::with_capacity(SETTLE + len);
    while out.len() < SETTLE + len {
        osc.start_block();
        osc.ensure_processed_to(BLOCK_SIZE);
        out.extend((0..BLOCK_SIZE).map(|i| osc.get_value_at("output", 0, i)));
    }
    out.drain(..SETTLE);
    out.truncate(len);
    out
}

/// Power outside the harmonics of `hz`, relative to the total, in dB.
fn alias_db(samples: &[f32], hz: f32) -> f64 {
    let n = samples.len();
    let mut spectrum: Vec<Complex<f64>> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            // 4-term Blackman-Harris: sidelobes below -92 dB.
            let t = std::f64::consts::TAU * i as f64 / (n - 1) as f64;
            let w =
                0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos();
            Complex::new(s as f64 * w, 0.0)
        })
        .collect();
    FftPlanner::new().plan_fft_forward(n).process(&mut spectrum);
    let bin_hz = SAMPLE_RATE as f64 / n as f64;
    let power: Vec<f64> = spectrum[..n / 2].iter().map(|c| c.norm_sqr()).collect();
    let mut harmonic = vec![false; n / 2];
    let mut k = 0.0;
    while k * hz as f64 <= SAMPLE_RATE as f64 / 2.0 {
        let centre = (k * hz as f64 / bin_hz).round() as isize;
        for bin in (centre - 6)..=(centre + 6) {
            if let Some(slot) = usize::try_from(bin).ok().and_then(|b| harmonic.get_mut(b)) {
                *slot = true;
            }
        }
        k += 1.0;
    }
    let total: f64 = power.iter().sum();
    let alias: f64 = power
        .iter()
        .zip(&harmonic)
        .filter(|(_, h)| !**h)
        .map(|(p, _)| p)
        .sum();
    10.0 * (alias / total).log10()
}

#[test]
fn bright_table_does_not_alias() {
    // Pitches inside and just above the fade into the next level, and at
    // octave-spaced points across the range.
    for hz in [110.0, 183.0, 196.0, 440.0, 1760.0, 3520.0, 7040.0] {
        let db = alias_db(&render(hz, FFT_SIZE), hz);
        assert!(db < -80.0, "{hz} Hz: alias power {db:.1} dB");
    }
}

#[test]
fn high_notes_keep_their_fundamental() {
    for hz in [13000.0, 15000.0, 20000.0] {
        let samples = render(hz, 4800);
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        assert!(
            rms > 1.0,
            "{hz} Hz: RMS {rms:.3} V, the fundamental is missing"
        );
    }
}
