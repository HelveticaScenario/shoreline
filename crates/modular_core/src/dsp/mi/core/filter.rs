//! Ports of `stmlib::OnePole` and `stmlib::Svf`.

use std::f64::consts::PI;

/// `OnePole::tan<FREQUENCY_DIRTY>`: `tan(π f)` for a frequency `f` in cycles
/// per sample, accurate below about f = 1/6.
#[inline(always)]
pub fn tan_dirty(f: f32) -> f32 {
    const A: f32 = (3.736e-01 * PI * PI * PI) as f32;
    f * (std::f32::consts::PI + A * f * f)
}

/// `OnePole::tan<FREQUENCY_FAST>`: `tan(π f)` for a frequency `f` in cycles
/// per sample, tuned for 16 Hz–16 kHz at 48 kHz.
#[inline(always)]
pub fn tan_fast(f: f32) -> f32 {
    const A: f32 = (3.260e-01f32 as f64 * PI * PI * PI) as f32;
    const B: f32 = (1.823e-01f32 as f64 * PI * PI * PI * PI * PI) as f32;
    let f2 = f * f;
    f * (std::f32::consts::PI + f2 * (A + B * f2))
}

/// `OnePole::tan<FREQUENCY_ACCURATE>`: `tan(π f)` for a frequency `f` in
/// cycles per sample, accurate across the audio range; finite and positive up
/// to f = 1/2.
#[inline(always)]
pub fn tan_accurate(f: f32) -> f32 {
    const A: f32 = (3.333314036e-01f32 as f64 * PI * PI * PI) as f32;
    const B: f32 = (1.333923995e-01f32 as f64 * PI * PI * PI * PI * PI) as f32;
    const C: f32 = (5.33740603e-02f32 as f64 * PI * PI * PI * PI * PI * PI * PI) as f32;
    const D: f32 = (2.900525e-03f32 as f64 * PI * PI * PI * PI * PI * PI * PI * PI * PI) as f32;
    const E: f32 =
        (9.5168091e-03f32 as f64 * PI * PI * PI * PI * PI * PI * PI * PI * PI * PI * PI) as f32;
    let f2 = f * f;
    f * (std::f32::consts::PI + f2 * (A + f2 * (B + f2 * (C + f2 * (D + f2 * E)))))
}

/// Topology-preserving one-pole filter.
#[derive(Clone, Copy, Default)]
pub struct OnePole {
    g: f32,
    gi: f32,
    state: f32,
}

impl OnePole {
    /// Sets the coefficient from `g = tan(π f)`.
    #[inline]
    pub fn set_g(&mut self, g: f32) {
        self.g = g;
        self.gi = 1.0 / (1.0 + g);
    }

    #[inline(always)]
    pub fn low_pass(&mut self, x: f32) -> f32 {
        let lp = (self.g * x + self.state) * self.gi;
        self.state = self.g * (x - lp) + lp;
        lp
    }

    #[inline(always)]
    pub fn high_pass(&mut self, x: f32) -> f32 {
        x - self.low_pass(x)
    }
}

/// Chamberlin-style state-variable filter with zero-delay feedback.
#[derive(Clone, Copy, Default)]
pub struct Svf {
    g: f32,
    r: f32,
    h: f32,
    state_1: f32,
    state_2: f32,
}

impl Svf {
    /// Sets the frequency from `g = tan(π f)` and the resonance `q`.
    #[inline]
    pub fn set_g_q(&mut self, g: f32, q: f32) {
        self.g = g;
        self.r = 1.0 / q;
        self.h = 1.0 / (1.0 + self.r * g + g * g);
    }

    #[inline(always)]
    pub fn low_pass(&mut self, x: f32) -> f32 {
        self.process(x).1
    }

    #[inline(always)]
    pub fn band_pass(&mut self, x: f32) -> f32 {
        self.process(x).0
    }

    #[inline(always)]
    pub fn high_pass(&mut self, x: f32) -> f32 {
        self.process_all(x).0
    }

    /// One sample: the band-pass and low-pass outputs.
    #[inline(always)]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        let (_, bp, lp) = self.process_all(x);
        (bp, lp)
    }

    /// One sample: the high-pass, band-pass and low-pass outputs.
    #[inline(always)]
    fn process_all(&mut self, x: f32) -> (f32, f32, f32) {
        let hp = (x - self.r * self.state_1 - self.g * self.state_1 - self.state_2) * self.h;
        let bp = self.g * hp + self.state_1;
        self.state_1 = self.g * hp + bp;
        let lp = self.g * bp + self.state_2;
        self.state_2 = self.g * bp + lp;
        (hp, bp, lp)
    }
}
