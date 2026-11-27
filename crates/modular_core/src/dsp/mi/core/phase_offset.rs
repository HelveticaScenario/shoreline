//! The `phaseOffset` the `$mi.*` oscillators share: a shift of the whole
//! oscillator in time, in cycles. A fresh voice starts at its offset; a moving
//! offset is travelled, the shortest way round, as extra frequency.

use crate::dsp::mi::core::phase::wrap;
use crate::dsp::utils::phase_delta;
use crate::types::{GlideMark, SwapGlide};

/// A voice's offset control. A patch edit glides it the short way round the
/// cycle, and a NaN or infinite value holds the last finite one.
#[derive(Clone, Copy, Default)]
pub struct PhaseOffset {
    glide: SwapGlide,
    /// Whether the voice has started at its offset; a voice carried across a
    /// patch update keeps running from where it is.
    seeded: bool,
}

impl PhaseOffset {
    /// This sample's offset in `[0, 1)`, from the param's value and glide mark.
    #[inline(always)]
    pub fn update(&mut self, (value, mark): (f32, GlideMark), sample_rate: f32) -> f32 {
        self.glide.update_cyclic(value, mark, sample_rate);
        wrap(*self.glide)
    }

    /// True once, the first time a voice asks: a fresh voice seeds its phases
    /// at the offset.
    #[inline(always)]
    pub fn seed(&mut self) -> bool {
        !std::mem::replace(&mut self.seeded, true)
    }
}

/// `shift` held so a phasor running at `frequency` stays within ±`max`. The
/// bounds always hold 0, so a phasor already past the limit never travels
/// further out, and a phase with no way left to go never moves.
#[inline(always)]
fn limit(shift: f32, frequency: f32, max: f32) -> f32 {
    shift.clamp((-max - frequency).min(0.0), (max - frequency).max(0.0))
}

/// How far a phase has travelled toward its offset.
#[derive(Clone, Copy, Default)]
pub struct Travel {
    /// The offset reached so far, in `[0, 1)`.
    travelled: f32,
}

impl Travel {
    /// Starts at `offset`, as a phase seeded there.
    #[inline(always)]
    pub fn seed(&mut self, offset: f32) {
        self.travelled = wrap(offset);
    }

    /// This substep's share of the shortest way to `target` (in `[0, 1)`) over
    /// the `left` substeps remaining, as a shift to add to `frequency`, held
    /// so the phasor stays within ±`max`. All in cycles per substep.
    #[inline(always)]
    pub fn step(&mut self, target: f32, left: usize, frequency: f32, max: f32) -> f32 {
        let shift = limit(
            phase_delta(self.travelled, target) / left as f32,
            frequency,
            max,
        );
        self.travelled = wrap(self.travelled + shift);
        shift
    }
}

/// A phase that keeps in time with a travelling one at `ratio` times its
/// frequency: each leader shift owes it `ratio` times as far, which it travels
/// as fast as its own limit allows, so it moves smoothly even where the
/// leader's offset wraps.
#[derive(Clone, Copy, Default)]
pub struct Follower {
    /// The way still owed, less whole cycles.
    behind: f32,
}

impl Follower {
    /// Owes nothing, as a phase seeded in time with its leader.
    #[inline(always)]
    pub fn seed(&mut self) {
        self.behind = 0.0;
    }

    /// This substep's shift for the follower running at `frequency`, after its
    /// leader shifted `lead`.
    #[inline(always)]
    pub fn step(&mut self, lead: f32, ratio: f32, frequency: f32, max: f32) -> f32 {
        let behind = self.behind + ratio * lead;
        let behind = behind - behind.trunc();
        let shift = limit(behind, frequency, max);
        self.behind = behind - shift;
        shift
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly::PolySignal;
    use crate::types::Signal;

    const MAX: f32 = 0.25;

    fn mark() -> GlideMark {
        PolySignal::mono(Signal::Volts(0.0)).glide_mark(0)
    }

    #[test]
    fn travel_takes_the_shortest_way_and_lands() {
        let mut travel = Travel::default();
        travel.seed(0.9);
        let shifts: Vec<f32> = (0..4).map(|n| travel.step(0.1, 4 - n, 0.0, MAX)).collect();
        assert!(shifts.iter().all(|&s| s > 0.0), "{shifts:?}");
        assert!((shifts.iter().sum::<f32>() - 0.2).abs() < 1e-6);
        assert!((travel.step(0.1, 1, 0.0, MAX)).abs() < 1e-6);
    }

    #[test]
    fn a_phase_with_no_way_left_never_shifts() {
        let mut travel = Travel::default();
        for frequency in [0.0, MAX, MAX + 1e-7, -MAX - 1e-7, 3.0] {
            assert_eq!(travel.step(0.0, 1, frequency, MAX).to_bits(), 0);
        }
        let mut follower = Follower::default();
        assert_eq!(follower.step(0.0, 3.2, MAX + 1e-7, MAX).to_bits(), 0);
    }

    #[test]
    fn travel_holds_the_phasor_to_its_limit() {
        let mut travel = Travel::default();
        let shift = travel.step(0.4, 1, 0.2, MAX);
        assert!((shift - 0.05).abs() < 1e-7, "{shift}");
    }

    #[test]
    fn follower_moves_by_the_ratio_and_catches_up() {
        let mut follower = Follower::default();
        // A leader shift of 0.1 owes 0.32 at ratio 3.2: a 0.25 limit pays
        // 0.25 now and the rest next substep.
        assert!((follower.step(0.1, 3.2, 0.0, MAX) - 0.25).abs() < 1e-6);
        assert!((follower.step(0.0, 3.2, 0.0, MAX) - 0.07).abs() < 1e-6);
        assert_eq!(follower.step(0.0, 3.2, 0.0, MAX), 0.0);
    }

    #[test]
    fn offset_wraps_holds_and_seeds_once() {
        let mut offset = PhaseOffset::default();
        assert!((offset.update((1.25, mark()), 48000.0) - 0.25).abs() < 1e-6);
        assert!((offset.update((f32::NAN, mark()), 48000.0) - 0.25).abs() < 1e-6);
        assert!((offset.update((f32::NEG_INFINITY, mark()), 48000.0) - 0.25).abs() < 1e-6);
        assert!(offset.seed());
        assert!(!offset.seed());
    }

    #[test]
    fn unpatched_offset_is_zero() {
        let mut offset = PhaseOffset::default();
        assert_eq!(offset.update((0.0, mark()), 48000.0).to_bits(), 0);
        assert_eq!(offset.update((f32::NAN, mark()), 48000.0).to_bits(), 0);
    }

    /// A patch edit across the wrap moves the offset the short way round:
    /// from 0.95 to 0.05 it travels forward a tenth of a cycle, never back.
    #[test]
    fn swapped_offset_glides_the_short_way_round() {
        let swap = |generation| GlideMark {
            generation,
            changed: u64::MAX,
            lane: 0,
            lanes: 1,
        };
        let mut offset = PhaseOffset::default();
        let mut prev = 0.0;
        for _ in 0..100 {
            prev = offset.update((0.95, swap(1)), 48000.0);
        }
        let mut travelled = 0.0;
        for _ in 0..2000 {
            let now = offset.update((0.05, swap(2)), 48000.0);
            let step = phase_delta(prev, now);
            assert!(step >= 0.0, "moved back by {step}");
            travelled += step;
            prev = now;
        }
        assert!((travelled - 0.1).abs() < 1e-4, "travelled {travelled}");
        assert!((prev - 0.05).abs() < 1e-6, "ended on {prev}");
    }
}
