//! Ports of `stmlib::OnePole`, `stmlib::Svf` and `stmlib::NaiveSvf`.

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
#[expect(
    clippy::excessive_precision,
    reason = "stmlib's coefficients, verbatim"
)]
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

    /// `Svf::ProcessMultimodeLPtoHP`: one sample swept from low-pass (`mode`
    /// 0) through band-pass (0.5) to high-pass (1).
    #[inline(always)]
    pub fn low_to_high_pass(&mut self, x: f32, mode: f32) -> f32 {
        let hp_gain = (-mode * 2.0 + 1.0).min(0.0);
        let bp_gain = 1.0 - 2.0 * (mode - 0.5).abs();
        let lp_gain = (1.0 - mode * 2.0).max(0.0);
        let (hp, bp, lp) = self.process_all(x);
        hp_gain * hp + bp_gain * bp + lp_gain * lp
    }

    /// One sample: the low-pass and high-pass outputs.
    #[inline(always)]
    pub fn low_and_high_pass(&mut self, x: f32) -> (f32, f32) {
        let (hp, _, lp) = self.process_all(x);
        (lp, hp)
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

/// `stmlib::NaiveSvf`: the explicit Chamberlin state-variable filter.
#[derive(Clone, Copy, Default)]
pub struct NaiveSvf {
    f: f32,
    damp: f32,
    lp: f32,
    bp: f32,
}

/// The frequency coefficient and damping of the filter that
/// `set_f_q<FREQUENCY_DIRTY>(f, resonance)` makes at 48 kHz, for a rate
/// `to_48k` times 48 kHz: `f` in cycles per 48 kHz sample, held at 0.158.
///
/// The dirty coefficient `2π f` warps the filter far above `f` near that
/// limit, so at another rate the filter's poles are carried over in hertz
/// (each 48 kHz pole raised to the power `1 / to_48k`) rather than its
/// nominal cutoff. At 48 kHz this is the stmlib filter itself.
pub fn naive_svf_coefficients(f: f32, resonance: f32, to_48k: f32) -> (f32, f32) {
    let f = 2.0 * std::f32::consts::PI * f.min(0.158);
    let damp = 1.0 / resonance;
    if to_48k == 1.0 {
        return (f, damp);
    }
    // The 48 kHz filter's poles, the roots of z² + a1 z + a2.
    let (f, damp) = (f64::from(f), f64::from(damp));
    let a1 = f * f + f * damp - 2.0;
    let a2 = 1.0 - f * damp;
    let radius = a2.max(0.0).sqrt();
    let angle = (-a1 / (2.0 * radius)).clamp(-1.0, 1.0).acos();
    let exponent = 1.0 / f64::from(to_48k);
    let radius = radius.powf(exponent);
    // A pole the rate cannot hold stays just below its Nyquist.
    let angle = (angle * exponent).min(0.95 * std::f64::consts::PI);
    let a1 = -2.0 * radius * angle.cos();
    let a2 = radius * radius;
    let f = (1.0 + a1 + a2).max(f64::MIN_POSITIVE).sqrt();
    (f as f32, ((1.0 - a2) / f) as f32)
}

impl NaiveSvf {
    /// Sets the coefficients [`naive_svf_coefficients`] gives.
    #[inline(always)]
    pub fn set_coefficients(&mut self, (f, damp): (f32, f32)) {
        self.f = f;
        self.damp = damp;
    }

    #[inline(always)]
    pub fn low_pass(&mut self, x: f32) -> f32 {
        let notch = x - self.bp * self.damp;
        self.lp += self.f * self.bp;
        let hp = notch - self.lp;
        self.bp += self.f * hp;
        self.lp
    }
}
