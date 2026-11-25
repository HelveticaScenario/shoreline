//! The `blend` output of a module with Plaits' main and aux outputs: the two
//! crossfaded by `mix`, 0 V the main alone and 5 V the aux alone.

use crate::types::{GlideMark, SwapGlide};

/// One lane's crossfade, gliding `mix` across patch updates.
#[derive(Clone, Copy, Default, SeedSide)]
pub struct Blend {
    mix: SwapGlide,
}

impl Blend {
    /// `main` crossfaded into `aux` by `mix` volts. Exactly `main` at 0 V and
    /// exactly `aux` at 5 V; a NaN `mix` holds the last finite one.
    #[inline(always)]
    pub fn step(
        &mut self,
        main: f32,
        aux: f32,
        (mix, mark): (f32, GlideMark),
        sample_rate: f32,
    ) -> f32 {
        let t = self.position((mix, mark), sample_rate);
        crossfade(main, aux, t)
    }

    /// Where `mix` volts sits between the main sound, 0, and the aux, 1.
    #[inline(always)]
    pub fn position(&mut self, (mix, mark): (f32, GlideMark), sample_rate: f32) -> f32 {
        self.mix.update(mix.clamp(0.0, 5.0), mark, sample_rate);
        *self.mix * 0.2
    }
}

/// `main` crossfaded into `aux` at a [`Blend::position`]: `main` itself at 0
/// and `aux` itself at 1, whatever the other holds.
#[inline(always)]
pub fn crossfade(main: f32, aux: f32, position: f32) -> f32 {
    if position == 0.0 {
        main
    } else if position == 1.0 {
        aux
    } else {
        main * (1.0 - position) + aux * position
    }
}

#[cfg(test)]
mod tests {
    use super::Blend;
    use crate::poly::PolySignal;
    use crate::types::Signal;

    fn mark() -> crate::types::GlideMark {
        PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
    }

    #[test]
    fn ends_pass_one_output_through() {
        let mut blend = Blend::default();
        assert_eq!(blend.step(0.3, -0.7, (0.0, mark()), 48000.0), 0.3);
        assert_eq!(blend.step(0.3, -0.7, (5.0, mark()), 48000.0), -0.7);
        assert_eq!(blend.step(0.3, -0.7, (9.0, mark()), 48000.0), -0.7);
        assert_eq!(blend.step(0.3, -0.7, (-2.0, mark()), 48000.0), 0.3);
    }

    #[test]
    fn nan_mix_holds_the_last_finite_one() {
        let mut blend = Blend::default();
        assert_eq!(blend.step(0.3, -0.7, (f32::NAN, mark()), 48000.0), 0.3);
        assert_eq!(blend.step(0.3, -0.7, (5.0, mark()), 48000.0), -0.7);
        assert_eq!(blend.step(0.3, -0.7, (f32::NAN, mark()), 48000.0), -0.7);
    }

    #[test]
    fn midpoint_averages() {
        let mut blend = Blend::default();
        assert!((blend.step(1.0, -1.0, (2.5, mark()), 48000.0)).abs() < 1e-6);
    }
}
