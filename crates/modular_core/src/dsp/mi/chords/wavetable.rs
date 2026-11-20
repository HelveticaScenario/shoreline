//! `plaits::WavetableOscillator<128, 15>` on the chord engine's fifteen waves
//! from Plaits' integrated wavetables.

use crate::dsp::mi::core::{dsp::one_pole, tables::WAV_INTEGRATED_WAVES};

const WAVE_SIZE: usize = 128;
/// Each integrated wave's length in the table, with the samples past its end
/// that interpolation reads.
const WAVE_STRIDE: usize = 132;
const NUM_WAVES: usize = 15;

/// The start of the wave at `bank`, `row` and `column` of Plaits' 8×8 banks.
const fn wave(bank: usize, row: usize, column: usize) -> usize {
    (bank * 64 + row * 8 + column) * WAVE_STRIDE
}

/// The waves `waveform` scans, in order.
const WAVES: [usize; NUM_WAVES] = [
    wave(2, 6, 1),
    wave(2, 6, 6),
    wave(2, 6, 4),
    wave(0, 6, 0),
    wave(0, 6, 1),
    wave(0, 6, 2),
    wave(0, 6, 7),
    wave(2, 4, 7),
    wave(2, 4, 6),
    wave(2, 4, 5),
    wave(2, 4, 4),
    wave(2, 4, 3),
    wave(2, 4, 2),
    wave(2, 4, 1),
    wave(2, 4, 0),
];

/// The integrated wave starting at `start`, read `fractional` of the way from
/// sample `integral` to the next.
#[inline(always)]
fn read(start: usize, integral: usize, fractional: f32) -> f32 {
    let a = f32::from(WAV_INTEGRATED_WAVES[start + integral]);
    let b = f32::from(WAV_INTEGRATED_WAVES[start + integral + 1]);
    a + (b - a) * fractional
}

/// The waves are stored integrated; differentiating them a sample at a time
/// band-limits them, and a one-pole at the same cutoff smooths what remains.
#[derive(Clone, Copy, Default)]
pub struct WavetableVoice {
    phase: f32,
    /// The last integrated sample; `None` before the first, which has no
    /// difference to take.
    previous: Option<f32>,
    differentiated: f32,
    lp: f32,
}

impl WavetableVoice {
    /// One sample at `frequency` cycles per sample (within ±0.5, and at
    /// least 1e-7 either way; negative runs the wave backward) at
    /// `amplitude`, scanning the waves at `waveform` (0–1). `frequency_48k`
    /// is the same pitch in Plaits' 48 kHz terms.
    #[inline(always)]
    pub fn step(
        &mut self,
        frequency: f32,
        frequency_48k: f32,
        amplitude: f32,
        waveform: f32,
    ) -> f32 {
        let amplitude = amplitude * (1.0 - 2.0 * frequency_48k.abs());
        // Each difference is scaled by the pitch that made it, so the filters
        // hold the wave's level when the pitch swings. Dividing by the signed
        // pitch also undoes the sign a backward wave gives its differences.
        let scale = 1.0 / (frequency * 131072.0);
        // A coefficient relative to the pitch keeps its time constant in
        // cycles of the wave, the same at any rate.
        let cutoff = (WAVE_SIZE as f32 * frequency.abs()).min(1.0);
        self.phase += frequency;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        } else if self.phase < 0.0 {
            self.phase += 1.0;
            // A phase a hair below 0 wraps to 1, which is 0.
            if self.phase >= 1.0 {
                self.phase = 0.0;
            }
        }
        let waveform = waveform * (NUM_WAVES as f32 - 1.0001);
        let wave = waveform as usize;
        let blend = waveform - wave as f32;
        let p = self.phase * WAVE_SIZE as f32;
        let integral = p as usize;
        let fractional = p - integral as f32;
        let x0 = read(WAVES[wave], integral, fractional);
        let x1 = read(WAVES[wave + 1], integral, fractional);
        let s = x0 + (x1 - x0) * blend;
        let previous = self.previous.unwrap_or(s);
        one_pole(&mut self.differentiated, (s - previous) * scale, cutoff);
        self.previous = Some(s);
        one_pole(&mut self.lp, self.differentiated, cutoff);
        amplitude * self.lp
    }
}
