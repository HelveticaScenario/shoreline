//! `plaits::Particle`: random impulses, each tick's first one retuning a
//! resonant band-pass that rings on them.

use crate::dsp::mi::core::{
    filter::{Svf, tan_dirty},
    random::Random,
    units::semitones_to_ratio,
};

/// The most samples one tick spans: 64 holds a 4 kHz tick up to 256 kHz.
pub const MAX_TICK: usize = 64;

#[derive(Clone, Copy)]
pub struct Particle {
    filter: Svf,
    pre_gain: f32,
    /// This tick's impulse draw for each of its samples, and the draw that
    /// retunes the filter at the tick's first impulse.
    draws: [f32; MAX_TICK],
    first_impulse: usize,
    frequency_draw: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            filter: Svf::default(),
            pre_gain: 0.0,
            draws: [1.0; MAX_TICK],
            first_impulse: usize::MAX,
            frequency_draw: 0.0,
        }
    }
}

impl Particle {
    /// Draws a tick of `samples` samples, in the order Plaits draws a block:
    /// the first impulse draw (replaced by `density` on a sync, so the
    /// particle fires), the frequency draw at the first impulse, and each
    /// next impulse draw, the last of them unused.
    pub fn draw_tick(&mut self, samples: usize, density: f32, sync: bool, random: &mut Random) {
        let mut u = random.float();
        if sync {
            u = density;
        }
        self.first_impulse = usize::MAX;
        for (k, draw) in self.draws[..samples].iter_mut().enumerate() {
            *draw = u;
            if u <= density && self.first_impulse == usize::MAX {
                self.frequency_draw = random.float();
                self.first_impulse = k;
            }
            u = random.float();
        }
    }

    /// Sample `k` of the tick: the impulse, and the filter ringing on it.
    /// `density` and `gain` are per sample at this rate; `f0` is in cycles
    /// per sample at 48 kHz, `to_48k` this rate over 48 kHz, and
    /// `density_48k` Plaits' density, which sets the filter's gain.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        k: usize,
        density: f32,
        gain: f32,
        f0: f32,
        spread: f32,
        q: f32,
        density_48k: f32,
        to_48k: f32,
    ) -> (f32, f32) {
        let u = self.draws[k];
        let s = if u <= density { u * gain } else { 0.0 };
        if k == self.first_impulse {
            let detune = 2.0 * self.frequency_draw - 1.0;
            let f = (semitones_to_ratio(spread * detune) * f0).min(0.25);
            self.pre_gain = 0.5 / (q * f * density_48k.sqrt()).sqrt();
            self.filter.set_g_q(tan_dirty((f / to_48k).min(0.25)), q);
        }
        // An impulse carries the same energy into the filter at any rate.
        (s, self.filter.band_pass(self.pre_gain * s * to_48k))
    }
}
