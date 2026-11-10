//! `plaits::WaveTerrainEngine`'s surfaces: heights over the square
//! (x, y) ∈ [−1, 1]², five closed-form and three from Plaits' wavetables.

use crate::dsp::mi::core::{sine::sine, tables::WAV_INTEGRATED_WAVES};

/// Surfaces to scan.
pub const TERRAINS: usize = 8;

/// Samples per wave in the wavetables, and as stored (with 4 wrapped).
const WAVE_SIZE: usize = 128;
const WAVE_STRIDE: usize = WAVE_SIZE + 4;
/// Waves per wavetable bank.
const WAVES: usize = 64;

fn squash(x: f32, a: f32) -> f32 {
    let x = x * a;
    x / (1.0 + x.abs())
}

/// The derivative of a stored, integrated wave between samples `i + 1` and
/// `i + 2`, at `t` of the way across.
#[inline(always)]
fn integrated_wave(wave: &[i16], i: usize, t: f32) -> f32 {
    let a = f32::from(wave[i]);
    let b = f32::from(wave[i + 1]);
    let c = f32::from(wave[i + 2]);
    (b - a) + (c - b - b + a) * t
}

/// `TerrainLookupWT`: a wavetable bank as a surface, `y` across a wave and
/// `x` across the bank's waves. Coordinates past the square read its edge.
fn wavetable(x: f32, y: f32, bank: usize) -> f32 {
    let sample = ((y + 1.0) * 0.5 * WAVE_SIZE as f32).clamp(0.0, WAVE_SIZE as f32);
    let wave = ((x + 1.0) * 0.5 * (WAVES - 1) as f32).clamp(0.0, (WAVES - 1) as f32);
    let sample_integral = sample as usize;
    let sample_fractional = sample - sample_integral as f32;
    let wave_integral = (wave as usize).min(WAVES - 2);
    let wave_fractional = wave - wave_integral as f32;

    let start = (bank * WAVES + wave_integral) * WAVE_STRIDE;
    let a = integrated_wave(
        &WAV_INTEGRATED_WAVES[start..start + WAVE_STRIDE],
        sample_integral,
        sample_fractional,
    );
    let start = start + WAVE_STRIDE;
    let b = integrated_wave(
        &WAV_INTEGRATED_WAVES[start..start + WAVE_STRIDE],
        sample_integral,
        sample_fractional,
    );
    (a + (b - a) * wave_fractional) * (1.0 / 1024.0)
}

/// The height of surface `index` (below `TERRAINS`) at (x, y).
#[inline(always)]
pub fn terrain(x: f32, y: f32, index: usize) -> f32 {
    // Plaits' table sine reads positive phases, hence the offset.
    const K: f32 = 4.0;
    match index {
        0 => (squash(sine(K + x * 1.273), 2.0) - sine(K + y * (x + 1.571) * 0.637)) * 0.57,
        1 => {
            let xy = x * y;
            sine(K + sine(K + (x + y) * 0.637) / (0.2 + xy * xy) * 0.159)
        }
        2 => {
            let xy = x * y;
            sine(K + sine(K + 2.387 * xy) / (0.350 + xy * xy) * 0.159)
        }
        3 => {
            let xy = x * y;
            let xys = (x - 0.25) * (y + 0.25);
            sine(K + xy / (2.0 + (5.0 * xys).abs()) * 6.366)
        }
        4 => sine(
            0.159 / (0.170 + (y - 0.25).abs())
                + 0.477 / (0.350 + ((x + 0.5) * (y + 1.5)).abs())
                + K,
        ),
        _ => wavetable(x, y, TERRAINS - 1 - index),
    }
}
