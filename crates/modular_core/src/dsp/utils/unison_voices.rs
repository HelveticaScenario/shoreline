//! Where the voices of a unison stack sit: their lanes, their detune and
//! their place in the stereo field. `$unison`, `$supersaw` and `$stereoFold`
//! share these, so a stack built from `$unison` folds to the same image as a
//! `$supersaw`.

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

/// The voice (by detune order) placed at stereo slot `slot` of `voices`.
/// The left half of the slots takes the even detunes and the right half the
/// odd ones, so each side holds a balanced spread of the whole detune range.
#[inline]
pub fn detune_of_slot(slot: usize, voices: usize) -> usize {
    let half = voices.div_ceil(2);
    if slot < half {
        slot * 2
    } else {
        (slot - half) * 2 + 1
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

    #[test]
    fn slots_hold_every_voice_once() {
        for voices in 1..=64 {
            let mut seen = vec![false; voices];
            for slot in 0..voices {
                let d = detune_of_slot(slot, voices);
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
    fn each_side_spans_the_detune_range() {
        // Five voices: left takes detunes 0, 2, 4 and right 1, 3.
        let order: Vec<usize> = (0..5).map(|s| detune_of_slot(s, 5)).collect();
        assert_eq!(order, [0, 2, 4, 1, 3]);
    }
}
