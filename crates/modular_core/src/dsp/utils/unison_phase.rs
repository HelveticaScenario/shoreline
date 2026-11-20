//! Starting phases for a unison stack whose copies are detuned evenly, as
//! `$unison` and `$supersaw` detune them.

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
    let delta = (target - current + 0.5).rem_euclid(1.0) - 0.5;
    if delta.abs() <= max_step {
        target
    } else {
        (current + max_step.copysign(delta)).rem_euclid(1.0)
    }
}

/// The phase each slot of a unison stack holds, carried across patch updates
/// so a count change slides the phases to their new targets instead of
/// jumping, which would click.
pub struct UnisonPhases {
    phase: [f32; PORT_MAX_LANES],
    /// Slots an earlier sample wrote, here or in the module this state came
    /// from. Slots at or past it start on their target.
    held: usize,
}

impl Default for UnisonPhases {
    fn default() -> Self {
        Self {
            phase: [0.0; PORT_MAX_LANES],
            held: 0,
        }
    }
}

impl UnisonPhases {
    /// Moves `slot` one sample toward the phase of copy `voice` of `count`,
    /// by at most `max_step`, and returns it.
    #[inline]
    pub fn step(&mut self, slot: usize, voice: usize, count: usize, max_step: f32) -> f32 {
        let target = voice_phase(voice, count);
        let phase = &mut self.phase[slot];
        *phase = if slot < self.held {
            glide_phase(*phase, target, max_step)
        } else {
            target
        };
        *phase
    }

    /// Records that this sample wrote the first `slots` slots.
    #[inline]
    pub fn hold(&mut self, slots: usize) {
        self.held = slots;
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
    fn unwritten_slots_start_on_target_and_held_ones_glide() {
        let mut phases = UnisonPhases::default();
        assert_eq!(phases.step(2, 2, 4, 0.01), 0.5);
        phases.hold(3);
        // Copy 2 of 3 sits at 4/6; slot 2 moves one step toward it.
        assert!((phases.step(2, 2, 3, 0.01) - 0.51).abs() < 1e-6);
        // Slot 3 was never written, so it starts on copy 3 of 4's phase.
        assert_eq!(phases.step(3, 3, 4, 0.01), 0.125);
    }
}
