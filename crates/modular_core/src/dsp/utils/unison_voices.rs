//! Where the voices of a unison stack sit: their lanes, their detune and
//! their place in the stereo field. `$unison`, `$supersaw` and `$stereoFold`
//! share these, so a stack built from `$unison` folds to the same image as a
//! `$supersaw`; `$pan` spreads its lanes by [`centred_position`] too.

use crate::poly::PORT_MAX_LANES;

/// The lane of copy `voice` of note `note` among `notes`: copies are laid out
/// voice-major, each a full set of the notes.
#[inline]
pub fn unison_lane(voice: usize, note: usize, notes: usize) -> usize {
    voice * notes + note
}

/// Where voice `voice` of `voices` sits in the detune spread, 0 lowest to 1
/// highest; a lone voice sits in the middle.
#[inline]
pub fn detune_position(voice: usize, voices: usize) -> f32 {
    if voices > 1 {
        voice as f32 / (voices - 1) as f32
    } else {
        0.5
    }
}

/// Which voice, by detune order, sits at each stereo slot of a stack, left
/// to right. Mirrored slots hold mirrored detunes and an odd stack's
/// undetuned voice sits in the centre slot. The pairs are placed so that
/// each side's loudness-weighted pitch sits as near the stack's centre as
/// the voice count allows: exactly for 4 or 5 voices, 8 or 9 and so on. Two
/// or three voices leave the left side flat and the right side sharp.
#[derive(Clone, Copy)]
pub struct SlotOrder([u8; PORT_MAX_LANES]);

impl Default for SlotOrder {
    fn default() -> Self {
        Self::new(1)
    }
}

impl SlotOrder {
    /// Outer slot pair `k` takes detune pair `j`, the inner detunes going to
    /// the outer slots, and each pair, heaviest first, leans the left side
    /// against what it carries so far.
    pub fn new(voices: usize) -> Self {
        let voices = voices.clamp(1, PORT_MAX_LANES);
        let pairs = voices / 2;
        let centre = (voices - 1) as f32 / 2.0;
        // How far a pair leans the left side: its detune times its slots'
        // pan offset, flat with its lower voice on the left and sharp the
        // other way round.
        let mut weights = [(0.0f32, 0usize); PORT_MAX_LANES / 2];
        for (k, weight) in weights.iter_mut().enumerate().take(pairs) {
            let j = pairs - 1 - k;
            *weight = ((centre - j as f32) * (centre - k as f32), k);
        }
        weights[..pairs].sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut order = [0u8; PORT_MAX_LANES];
        let mut lean = 0.0f32;
        for &(weight, k) in &weights[..pairs] {
            let (low, high) = (pairs - 1 - k, voices - pairs + k);
            let (left, right) = if lean >= 0.0 {
                lean -= weight;
                (low, high)
            } else {
                lean += weight;
                (high, low)
            };
            order[k] = left as u8;
            order[voices - 1 - k] = right as u8;
        }
        if voices % 2 == 1 {
            order[pairs] = pairs as u8;
        }
        Self(order)
    }

    /// The voice, by detune order, at stereo slot `slot`.
    #[inline]
    pub fn detune(&self, slot: usize) -> usize {
        self.0[slot] as usize
    }
}

/// Item `index` of `count` spread evenly from -1 to 1: a voice's detune or a
/// stereo slot's pan position. A lone item sits at 0.
#[inline]
pub fn centred_position(index: usize, count: usize) -> f32 {
    2.0 * detune_position(index, count) - 1.0
}

/// Gain that keeps a stack of `voices` uncorrelated voices at one voice's
/// loudness.
#[inline]
pub fn unison_gain(voices: usize) -> f32 {
    1.0 / (voices.max(1) as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dsp::core::pan::pan_pair;

    /// How far the left side's pitch, weighted by each slot's power into it
    /// at full width, sits from the stack's centre, as a fraction of the
    /// detune range.
    fn left_pull(order: &SlotOrder, voices: usize) -> f32 {
        if voices < 2 {
            return 0.0;
        }
        let centre = (voices - 1) as f32 / 2.0;
        let (mut pull, mut power) = (0.0, 0.0);
        for slot in 0..voices {
            // A centred pair panned to `p` sends `1 - p` of its power left.
            let left = 1.0 - centred_position(slot, voices);
            pull += left * (order.detune(slot) as f32 - centre);
            power += left;
        }
        pull / power / (voices - 1) as f32
    }

    #[test]
    fn slots_hold_every_voice_once() {
        for voices in 1..=64 {
            let order = SlotOrder::new(voices);
            let mut seen = vec![false; voices];
            for slot in 0..voices {
                let d = order.detune(slot);
                assert!(!seen[d], "{voices}: voice {d} placed twice");
                seen[d] = true;
            }
        }
    }

    #[test]
    fn slots_are_symmetric_about_the_centre() {
        for voices in 1..=64 {
            for slot in 0..voices {
                let mirrored = centred_position(voices - 1 - slot, voices);
                assert!((centred_position(slot, voices) + mirrored).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn the_pull_weights_slots_by_the_pan_law() {
        for slot in 0..7 {
            let [left, _] = pan_pair([1.0, 1.0], centred_position(slot, 7));
            assert!((left * left - (1.0 - centred_position(slot, 7))).abs() < 1e-5);
        }
    }

    #[test]
    fn each_side_centres_its_pitch() {
        for voices in 2..=64 {
            let order = SlotOrder::new(voices);
            let pull = left_pull(&order, voices).abs();
            let bound = match voices {
                2 => 0.5,
                3 => 1.0 / 3.0,
                _ if (voices / 2) % 2 == 0 => 1e-5,
                _ => 0.05,
            };
            assert!(pull <= bound + 1e-6, "{voices}: {pull}");
        }
    }

    #[test]
    fn odd_stacks_centre_the_undetuned_voice() {
        for voices in (1..=63).step_by(2) {
            assert_eq!(SlotOrder::new(voices).detune(voices / 2), voices / 2);
        }
    }

    #[test]
    fn five_voices_pair_inner_detunes_with_outer_slots() {
        let order = SlotOrder::new(5);
        let detunes: Vec<usize> = (0..5).map(|s| order.detune(s)).collect();
        assert_eq!(detunes, [1, 4, 2, 0, 3]);
    }
}
