//! The layout of a unison stack: a note played as several detuned copies,
//! its voices. Voices are numbered by pitch, 0 the flattest; stereo slots,
//! the pan positions they are spread over, are numbered 0 hard left to the
//! last hard right. `$unison`, `$supersaw`, `$stereoFold` and `$pan` share
//! these helpers, so a `$unison` stack folded by `$stereoFold` is panned the
//! same way as a `$supersaw`.

use crate::poly::PORT_MAX_LANES;

/// The lane holding voice `voice` of note `note` when a stack of `notes`
/// notes is laid out across lanes: every note's voice 0 first, then every
/// note's voice 1, and so on.
#[inline]
pub fn unison_lane(voice: usize, note: usize, notes: usize) -> usize {
    voice * notes + note
}

/// Voice `voice`'s place in the detune spread, 0 for the flattest voice to 1
/// for the sharpest; a lone voice sits at 0.5.
#[inline]
pub fn detune_position(voice: usize, voices: usize) -> f32 {
    if voices > 1 {
        voice as f32 / (voices - 1) as f32
    } else {
        0.5
    }
}

/// Voice `voice`'s place in the detune spread, from -0.5 for the flattest
/// voice to 0.5 for the sharpest; a lone voice sits at 0.
#[inline]
pub fn spread_position(voice: usize, voices: usize) -> f32 {
    detune_position(voice, voices) - 0.5
}

/// How far, in octaves, the voice at `position` in the detune spread sits
/// from the note when the flattest and sharpest voices are `semitones`
/// apart.
#[inline(always)]
pub fn detune_octaves(position: f32, semitones: f32) -> f32 {
    position * semitones / 12.0
}

/// Which voice plays in each stereo slot.
///
/// Slots and voices both pair up from the outside in: slot `k` with its
/// mirror slot, and a voice detuned flat with the one detuned sharp by the
/// same amount. Each slot pair holds one voice pair, one voice on each
/// side, and an odd stack's in-tune voice takes the centre slot. Which way
/// round each pair goes is chosen so that the left speaker, heard on its
/// own, averages on the note (the right speaker mirrors it). That works out
/// exactly for 4 or 5 voices, 8 or 9 and so on, and nearly for other counts;
/// 2 or 3 voices always leave the left side flat and the right side sharp.
#[derive(Clone, Copy)]
pub struct SlotOrder([u8; PORT_MAX_LANES]);

impl Default for SlotOrder {
    fn default() -> Self {
        Self::new(1)
    }
}

impl SlotOrder {
    pub fn new(voices: usize) -> Self {
        let voices = voices.clamp(1, PORT_MAX_LANES);
        let pairs = voices / 2;
        let centre = (voices - 1) as f32 / 2.0;
        // Slot pair `k` (counted from the outside) holds voice pair `j`
        // (counted from the most detuned): the most detuned voices take the
        // slots nearest the centre, which evens out how far each pair pulls
        // so the pulls can cancel. A pair's pull on the left side's average
        // pitch is its detune times its slots' distance from the centre:
        // flat with its flat voice on the left, sharp the other way round.
        let mut weights = [(0.0f32, 0usize); PORT_MAX_LANES / 2];
        for (k, weight) in weights.iter_mut().enumerate().take(pairs) {
            let j = pairs - 1 - k;
            *weight = ((centre - j as f32) * (centre - k as f32), k);
        }
        weights[..pairs].sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut order = [0u8; PORT_MAX_LANES];
        // Strongest pull first, each pair goes whichever way brings the left
        // side's running pull back towards zero.
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

    /// The voice, numbered by pitch, that plays in slot `slot`.
    #[inline]
    pub fn detune(&self, slot: usize) -> usize {
        self.0[slot] as usize
    }
}

/// Item `index` of `count` spread evenly from -1 to 1: a voice's detune or a
/// slot's pan position. A lone item sits at 0.
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

    /// How far the left speaker's average pitch, weighting each voice by
    /// its power on the left at full width, sits from the note, as a
    /// fraction of the detune range.
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
