//! `stmlib::Random` and Plaits' `Dust`: one generator per voice, drawing
//! stmlib's sequence for the same seed.

#[derive(Clone, Copy)]
pub struct Random {
    state: u32,
}

impl Random {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    #[inline(always)]
    pub fn word(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        self.state
    }

    /// Uniform in `[0, 1)`.
    #[inline(always)]
    pub fn float(&mut self) -> f32 {
        self.word() as f32 / 4294967296.0
    }

    /// Plaits' `Dust`: with probability `density` per sample an impulse of a
    /// uniform height in `[0, 1)`, otherwise 0.
    #[inline(always)]
    pub fn dust(&mut self, density: f32) -> f32 {
        let inv_density = 1.0 / density;
        let u = self.float();
        if u < density { u * inv_density } else { 0.0 }
    }
}
