//! `plaits::ChordBank`: Plaits' eleven four-note chords, one picked at a time
//! with hysteresis, and their inversions spread across five voices.

use super::units::semitones_to_ratio;

pub const NUM_NOTES: usize = 4;
/// One voice more than notes, so the note an inversion lifts can crossfade
/// between its two octaves.
pub const NUM_VOICES: usize = NUM_NOTES + 1;
pub const NUM_CHORDS: usize = 11;

/// Each chord's notes in semitones above the root: octave, fifth, sus4,
/// minor, minor 7th, minor 9th, minor 11th, 6/9, major 9th, major 7th and
/// major. The hundredths detune doubled notes so they beat.
const CHORDS: [[f32; NUM_NOTES]; NUM_CHORDS] = [
    [0.00, 0.01, 11.99, 12.00],
    [0.00, 7.00, 7.01, 12.00],
    [0.00, 5.00, 7.00, 12.00],
    [0.00, 3.00, 7.00, 12.00],
    [0.00, 3.00, 7.00, 10.00],
    [0.00, 3.00, 10.00, 14.00],
    [0.00, 3.00, 10.00, 17.00],
    [0.00, 2.00, 9.00, 16.00],
    [0.00, 4.00, 11.00, 14.00],
    [0.00, 4.00, 7.00, 11.00],
    [0.00, 4.00, 7.00, 12.00],
];

/// How far past halfway to the next chord the selection moves before it
/// changes, in chords.
const HYSTERESIS: f32 = 0.075;

/// The voices of an inversion: each voice's frequency ratio to the root and
/// its level, and a bit set for each voice playing the root.
#[derive(Clone, Copy, Default)]
pub struct Inversion {
    pub ratios: [f32; NUM_VOICES],
    pub amplitudes: [f32; NUM_VOICES],
    pub root_mask: u8,
}

#[derive(Clone, Copy)]
pub struct ChordBank {
    index: usize,
    /// The frequency ratio of each of the chord's notes to its root.
    ratios: [f32; NUM_NOTES],
}

fn ratios(index: usize) -> [f32; NUM_NOTES] {
    CHORDS[index].map(semitones_to_ratio)
}

impl Default for ChordBank {
    fn default() -> Self {
        Self {
            index: 0,
            ratios: ratios(0),
        }
    }
}

impl ChordBank {
    /// Picks the chord nearest `index` (0–10), keeping the current one until
    /// `index` passes the halfway point by `HYSTERESIS`. NaN keeps it.
    #[inline(always)]
    pub fn set_chord(&mut self, index: f32) {
        if index.is_nan() {
            return;
        }
        let sign = if index > self.index as f32 { -1.0 } else { 1.0 };
        let index =
            ((index + sign * HYSTERESIS + 0.5) as i32).clamp(0, NUM_CHORDS as i32 - 1) as usize;
        if index != self.index {
            self.index = index;
            self.ratios = ratios(index);
        }
    }

    pub fn chord_index(&self) -> usize {
        self.index
    }

    /// `ComputeChordInversion`: the chord's notes over five voices at
    /// `inversion` (0–1). Each fifth of the range rotates the chord once,
    /// lifting its lowest note an octave at a time in four crossfades.
    pub fn inversion(&self, inversion: f32) -> Inversion {
        const BASE_GAIN: f32 = 0.25;
        let notes = NUM_NOTES as i32;
        let voices = NUM_VOICES as i32;
        let inversion = inversion * (NUM_NOTES * NUM_VOICES) as f32;
        let integral = inversion as i32;
        let fractional = inversion - integral as f32;
        let rotations = integral / notes;
        let rotated_note = integral % notes;

        let mut result = Inversion::default();
        for (i, &ratio) in self.ratios.iter().enumerate() {
            let i = i as i32;
            let transposition = 0.25 * (1 << ((notes - 1 + integral - i) / notes)) as f32;
            let target = ((i - rotations + voices) % voices) as usize;
            let previous = ((target as i32 - 1 + voices) % voices) as usize;
            if i == rotated_note {
                result.ratios[target] = ratio * transposition;
                result.ratios[previous] = result.ratios[target] * 2.0;
                result.amplitudes[previous] = BASE_GAIN * fractional;
                result.amplitudes[target] = BASE_GAIN * (1.0 - fractional);
            } else if i < rotated_note {
                result.ratios[previous] = ratio * transposition;
                result.amplitudes[previous] = BASE_GAIN;
            } else {
                result.ratios[target] = ratio * transposition;
                result.amplitudes[target] = BASE_GAIN;
            }
            if i == 0 {
                if i >= rotated_note {
                    result.root_mask |= 1 << target;
                }
                if i <= rotated_note {
                    result.root_mask |= 1 << previous;
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_the_chord_near_the_halfway_point() {
        let mut bank = ChordBank::default();
        bank.set_chord(3.0);
        assert_eq!(bank.chord_index(), 3);
        bank.set_chord(3.55);
        assert_eq!(bank.chord_index(), 3);
        bank.set_chord(3.6);
        assert_eq!(bank.chord_index(), 4);
        bank.set_chord(3.45);
        assert_eq!(bank.chord_index(), 4);
        bank.set_chord(3.4);
        assert_eq!(bank.chord_index(), 3);
        bank.set_chord(f32::NAN);
        assert_eq!(bank.chord_index(), 3);
        bank.set_chord(f32::INFINITY);
        assert_eq!(bank.chord_index(), NUM_CHORDS - 1);
        bank.set_chord(-7.0);
        assert_eq!(bank.chord_index(), 0);
    }

    #[test]
    fn every_inversion_sounds_four_notes() {
        let mut bank = ChordBank::default();
        bank.set_chord(10.0);
        for step in 0..=200 {
            let inversion = bank.inversion(step as f32 / 200.0);
            let total: f32 = inversion.amplitudes.iter().sum();
            assert!(
                (total - 1.0).abs() < 1e-6,
                "inversion {step}: level {total}"
            );
            assert_ne!(inversion.root_mask, 0);
        }
    }
}
