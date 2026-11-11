//! `stmlib::Limiter`, which Plaits runs on engines whose output can overshoot:
//! a peak follower that pulls the gain down once the peak passes 1.

/// Per-sample peak-follower coefficients for a sample rate.
#[derive(Clone, Copy, Default)]
pub struct LimiterRates {
    attack: f32,
    release: f32,
}

impl LimiterRates {
    /// Plaits' 0.05 attack and 0.00002 release per sample at 48 kHz, as the
    /// same time constants at `sample_rate`.
    pub fn new(sample_rate: f32) -> Self {
        let per_sample = |c: f64| (1.0 - (1.0 - c).powf(48000.0 / f64::from(sample_rate))) as f32;
        Self {
            attack: per_sample(0.05),
            release: per_sample(0.00002),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Limiter {
    peak: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Self { peak: 0.5 }
    }
}

impl Limiter {
    #[inline(always)]
    pub fn process(&mut self, s: f32, rates: &LimiterRates) -> f32 {
        let error = s.abs() - self.peak;
        self.peak += if error > 0.0 {
            rates.attack
        } else {
            rates.release
        } * error;
        let gain = if self.peak <= 1.0 {
            1.0
        } else {
            1.0 / self.peak
        };
        s * gain * 0.8
    }
}
