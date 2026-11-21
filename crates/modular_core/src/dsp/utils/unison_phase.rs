//! Starting phases for a unison stack whose copies are detuned evenly, as
//! `$unison` and `$supersaw` detune them.

use crate::dsp::utils::phase_delta;
use crate::poly::PORT_MAX_LANES;

/// Phase of copy `voice` of `count`, in cycles, copies numbered from lowest
/// to highest pitch. Schroeder's quadratic phases keep the copies from all
/// lining up at once as their evenly spaced detune drifts them against each
/// other, so the stack holds a steady level.
pub fn voice_phase(voice: usize, count: usize) -> f32 {
    let voice = voice as u64;
    let count = count as u64;
    // `voice² / 2·count` mod 1, reduced in integers so it stays exact.
    ((voice * voice) % (2 * count)) as f32 / (2 * count) as f32
}

/// How fast a phase slides to a new target when the copy count changes, in
/// cycles per second. A phase slide is a brief pitch shift of this many Hz,
/// small enough to hide in the detune.
const PHASE_GLIDE_HZ: f32 = 2.0;

/// The most a phase moves in one sample at `sample_rate`.
#[inline]
pub fn phase_glide_step(sample_rate: f32) -> f32 {
    PHASE_GLIDE_HZ / sample_rate
}

/// Steps `current` toward `target` by at most `max_step`, the short way
/// around the cycle. Both are phases in `[0, 1)`.
fn glide_phase(current: f32, target: f32, max_step: f32) -> f32 {
    let delta = phase_delta(current, target);
    if delta.abs() <= max_step {
        target
    } else {
        (current + max_step.copysign(delta)).rem_euclid(1.0)
    }
}

/// Each copy's phase, carried across patch updates so a count change slides
/// the phases to their new targets; a phase that jumped would click.
pub struct UnisonPhases {
    phase: [f32; PORT_MAX_LANES],
    target: [f32; PORT_MAX_LANES],
    count: usize,
    /// Whether any copy is still short of its target.
    gliding: bool,
}

impl Default for UnisonPhases {
    fn default() -> Self {
        Self {
            phase: [0.0; PORT_MAX_LANES],
            target: [0.0; PORT_MAX_LANES],
            count: 0,
            gliding: false,
        }
    }
}

impl UnisonPhases {
    /// The copy count; 0 until the first `set_count`.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Sets the copy count, at most `PORT_MAX_LANES`, and returns whether it
    /// changed. Call from `on_patch_update`, after the state transfer: copies
    /// the earlier count held glide to their new targets, and added copies
    /// start on theirs.
    pub fn set_count(&mut self, count: usize) -> bool {
        let changed = count != self.count;
        if changed {
            for (voice, target) in self.target.iter_mut().enumerate().take(count) {
                *target = voice_phase(voice, count);
            }
            for voice in self.count.min(count)..count {
                self.phase[voice] = self.target[voice];
            }
            self.count = count;
            self.gliding = true;
        }
        changed
    }

    /// Every copy's phase this sample, each moved toward its target by at
    /// most `max_step`.
    #[inline]
    pub fn advance(&mut self, max_step: f32) -> &[f32; PORT_MAX_LANES] {
        if self.gliding {
            let mut gliding = false;
            for (phase, &target) in self.phase.iter_mut().zip(&self.target).take(self.count) {
                *phase = glide_phase(*phase, target, max_step);
                gliding |= *phase != target;
            }
            self.gliding = gliding;
        }
        &self.phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_phases_are_schroeder_phases() {
        for count in 1..=PORT_MAX_LANES {
            for voice in 0..count {
                let phase = voice_phase(voice, count);
                let want = ((voice * voice) as f64 / (2 * count) as f64).fract() as f32;
                assert!((0.0..1.0).contains(&phase), "{voice}/{count}: {phase}");
                assert!(
                    (phase - want).abs() < 1e-6,
                    "{voice}/{count}: {phase} != {want}"
                );
            }
        }
    }

    #[test]
    fn phase_glides_the_short_way() {
        assert!((glide_phase(0.9, 0.1, 0.05) - 0.95).abs() < 1e-6);
        assert!((glide_phase(0.1, 0.9, 0.05) - 0.05).abs() < 1e-6);
        assert_eq!(glide_phase(0.5, 0.52, 0.05), 0.52);
    }

    #[test]
    fn fresh_copies_start_on_target_and_held_ones_glide() {
        let mut phases = UnisonPhases::default();
        phases.set_count(3);
        assert_eq!(phases.advance(0.01)[..3], [0.0, 1.0 / 6.0, 4.0 / 6.0]);
        phases.set_count(4);
        let next = phases.advance(0.01);
        // Copy 2 moves one step from 4/6 toward 4/8; copy 3 is new.
        assert!((next[2] - (4.0 / 6.0 - 0.01)).abs() < 1e-6);
        assert_eq!(next[3], 0.125);
    }

    #[test]
    fn a_count_change_settles_on_the_new_targets() {
        let mut phases = UnisonPhases::default();
        phases.set_count(5);
        phases.advance(0.01);
        phases.set_count(7);
        for _ in 0..100 {
            phases.advance(0.01);
        }
        for voice in 0..7 {
            assert_eq!(phases.advance(0.01)[voice], voice_phase(voice, 7));
        }
        assert!(!phases.gliding);
    }
}
