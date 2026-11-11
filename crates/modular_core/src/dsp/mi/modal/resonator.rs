//! `plaits::Resonator`: 24 band-pass modes tuned to a stretched harmonic
//! series, summed with amplitudes that taper up the series.

use crate::dsp::mi::core::{
    filter::tan_fast, interpolate::linear, tables::LUT_STIFFNESS, units::semitones_to_ratio,
};

pub const MODES: usize = 24;
/// Modes summed together before joining the output, as Plaits batches them.
const BATCH: usize = 4;
/// Where along the struck body the excitation lands; sets the mode amplitudes.
const POSITION: f32 = 0.015;

/// A state-variable filter's coefficients for frequency `f` (cycles per
/// sample) and resonance `q`, as Plaits' `ResonatorSvf` computes them.
#[inline(always)]
pub fn svf_coefficients(f: f32, q: f32) -> (f32, f32, f32) {
    let g = tan_fast(f);
    let r = 1.0 / q;
    let h = 1.0 / (1.0 + r * g + g * g);
    (g, r + g, h)
}

#[derive(Clone, Copy)]
pub struct Resonator {
    amplitudes: [f32; MODES],
    /// Each mode's band-pass coefficients and output gain, one array per
    /// coefficient so the per-mode work vectorises.
    g: [f32; MODES],
    r_plus_g: [f32; MODES],
    h: [f32; MODES],
    gain: [f32; MODES],
    state_1: [f32; MODES],
    state_2: [f32; MODES],
    /// The pitch, structure, brightness and decay the coefficients were last
    /// computed for; `NaN` = never computed.
    key: [f32; 4],
}

impl Default for Resonator {
    fn default() -> Self {
        // Plaits' approximate cosine oscillator at POSITION, stepped once per
        // mode.
        let frequency = (POSITION - 0.25).abs();
        let coefficient = 16.0 * frequency * (1.0 - 2.0 * frequency);
        let mut y1 = coefficient * 0.25;
        let mut y0 = 0.5f32;
        let mut amplitudes = [0.0; MODES];
        for amplitude in &mut amplitudes {
            let value = y0;
            y0 = coefficient * y0 - y1;
            y1 = value;
            *amplitude = (value + 0.5) * 0.25;
        }
        Self {
            amplitudes,
            g: [0.0; MODES],
            r_plus_g: [0.0; MODES],
            h: [0.0; MODES],
            gain: [0.0; MODES],
            state_1: [0.0; MODES],
            state_2: [0.0; MODES],
            key: [f32::NAN; 4],
        }
    }
}

impl Resonator {
    /// Recomputes the modes when any input differs from the last call.
    /// `f0` is in cycles per sample; `to_48k` converts it to Plaits' 48 kHz
    /// terms for the resonance and the taper at the top of the band.
    #[inline(always)]
    fn tune(&mut self, f0: f32, structure: f32, brightness: f32, decay: f32, to_48k: f32) {
        let key = [f0, structure, brightness, decay];
        if key.map(f32::to_bits) == self.key.map(f32::to_bits) {
            return;
        }
        self.key = key;

        let mut stiffness = linear(&LUT_STIFFNESS, structure, 64.0);
        let f0 = f0 * nth_harmonic_compensation(3, stiffness);
        let mut harmonic = f0;
        let mut stretch_factor = 1.0;
        let q_sqrt = semitones_to_ratio(decay * 79.7);
        let mut q = 500.0 * q_sqrt * q_sqrt;
        let mut brightness = brightness * (1.0 - structure * 0.3);
        brightness *= 1.0 - decay * 0.3;
        let q_loss = brightness * (2.0 - brightness) * 0.85 + 0.15;

        // The partials' running sums are sequential and cheap; the
        // coefficients that follow depend on each mode alone, so they
        // vectorise.
        let mut frequency = [0.0f32; MODES];
        let mut resonance = [0.0f32; MODES];
        for (f, q_mode) in frequency.iter_mut().zip(&mut resonance) {
            *f = harmonic * stretch_factor;
            *q_mode = q;
            stretch_factor += stiffness;
            // Shrinking a negative stiffness keeps partials from folding back
            // below zero; a positive one adds partials near the top.
            stiffness *= if stiffness < 0.0 { 0.93 } else { 0.98 };
            harmonic += f0;
            q *= q_loss;
        }
        for i in 0..MODES {
            let at_48k = (frequency[i] * to_48k).min(0.499);
            (self.g[i], self.r_plus_g[i], self.h[i]) =
                svf_coefficients(frequency[i].min(0.499), 1.0 + at_48k * resonance[i]);
            self.gain[i] = self.amplitudes[i] * (1.0 - at_48k * 2.0);
        }
    }

    /// One sample of the modes' response to `input`.
    #[inline(always)]
    pub fn process(
        &mut self,
        f0: f32,
        structure: f32,
        brightness: f32,
        decay: f32,
        input: f32,
        to_48k: f32,
    ) -> f32 {
        self.tune(f0, structure, brightness, decay, to_48k);
        let mut weighted = [0.0f32; MODES];
        for i in 0..MODES {
            let g = self.g[i];
            let hp = (input - self.r_plus_g[i] * self.state_1[i] - self.state_2[i]) * self.h[i];
            let bp = g * hp + self.state_1[i];
            self.state_1[i] = g * hp + bp;
            let lp = g * bp + self.state_2[i];
            self.state_2[i] = g * bp + lp;
            weighted[i] = self.gain[i] * bp;
        }
        // Summed in Plaits' order: each batch of four, then the batches.
        let mut out = 0.0;
        for batch in weighted.chunks_exact(BATCH) {
            let mut sum = 0.0;
            for &w in batch {
                sum += w;
            }
            out += sum;
        }
        out
    }
}

/// The factor that keeps the `n`th stretched partial on the pitch.
fn nth_harmonic_compensation(n: usize, stiffness: f32) -> f32 {
    let mut stiffness = stiffness;
    let mut stretch_factor = 1.0;
    for _ in 0..n - 1 {
        stretch_factor += stiffness;
        stiffness *= if stiffness < 0.0 { 0.93 } else { 0.98 };
    }
    1.0 / stretch_factor
}
