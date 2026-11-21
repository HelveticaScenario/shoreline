//! `plaits::Arpeggiator`: steps through a chord's notes across one, two or
//! four octaves, up, down, up and down, or at random.

use crate::dsp::mi::core::random::Random;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum Mode {
    #[default]
    Up,
    Down,
    UpDown,
    Random,
}

impl Mode {
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Up,
            1 => Self::Down,
            2 => Self::UpDown,
            _ => Self::Random,
        }
    }
}

/// Draws a random step at most this many times before keeping the last one,
/// which differs from the current step with near certainty long before.
const MAX_DRAWS: usize = 64;

#[derive(Clone, Copy)]
pub struct Arpeggiator {
    pub mode: Mode,
    /// Octaves the pattern spans: 1, 2 or 4.
    pub range: i32,
    note: i32,
    octave: i32,
    direction: i32,
}

impl Default for Arpeggiator {
    fn default() -> Self {
        Self {
            mode: Mode::Up,
            range: 1,
            note: 0,
            octave: 0,
            direction: 1,
        }
    }
}

impl Arpeggiator {
    pub fn note(&self) -> usize {
        self.note as usize
    }

    pub fn octave(&self) -> u32 {
        self.octave as u32
    }

    /// Advances one step through `num_notes` notes (1–4).
    pub fn clock(&mut self, num_notes: usize, random: &mut Random) {
        let num_notes = num_notes as i32;
        if num_notes == 1 && self.range == 1 {
            self.note = 0;
            self.octave = 0;
            return;
        }

        if self.mode == Mode::Random {
            for _ in 0..MAX_DRAWS {
                let w = random.word();
                let octave = ((w >> 4) % self.range as u32) as i32;
                let note = ((w >> 20) % num_notes as u32) as i32;
                if octave != self.octave || note != self.note {
                    self.octave = octave;
                    self.note = note;
                    break;
                }
            }
            return;
        }

        match self.mode {
            Mode::Up => self.direction = 1,
            Mode::Down => self.direction = -1,
            _ => {}
        }
        self.note += self.direction;

        // Up-down turns at most once per step, so two passes always settle.
        for _ in 0..2 {
            let mut done = true;
            if self.note >= num_notes || self.note < 0 {
                self.octave += self.direction;
                self.note = if self.direction > 0 { 0 } else { num_notes - 1 };
            }
            if self.octave >= self.range || self.octave < 0 {
                self.octave = if self.direction > 0 {
                    0
                } else {
                    self.range - 1
                };
                if self.mode == Mode::UpDown {
                    self.direction = -self.direction;
                    self.note = if self.direction > 0 { 1 } else { num_notes - 2 };
                    self.octave = if self.direction > 0 {
                        0
                    } else {
                        self.range - 1
                    };
                    done = false;
                }
            }
            if done {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(mode: Mode, range: i32, num_notes: usize, count: usize) -> Vec<(usize, u32)> {
        let mut arp = Arpeggiator {
            mode,
            range,
            ..Arpeggiator::default()
        };
        let mut random = Random::new(1);
        (0..count)
            .map(|_| {
                arp.clock(num_notes, &mut random);
                (arp.note(), arp.octave())
            })
            .collect()
    }

    #[test]
    fn walks_the_patterns() {
        assert_eq!(
            steps(Mode::Up, 2, 3, 7),
            [(1, 0), (2, 0), (0, 1), (1, 1), (2, 1), (0, 0), (1, 0)]
        );
        assert_eq!(steps(Mode::Down, 1, 3, 4), [(2, 0), (1, 0), (0, 0), (2, 0)]);
        assert_eq!(
            steps(Mode::UpDown, 1, 3, 6),
            [(1, 0), (2, 0), (1, 0), (0, 0), (1, 0), (2, 0)]
        );
    }

    #[test]
    fn a_lone_note_stays_in_range() {
        for mode in [Mode::Up, Mode::Down, Mode::UpDown, Mode::Random] {
            for range in [1, 2, 4] {
                for (note, octave) in steps(mode, range, 1, 50) {
                    assert_eq!(note, 0, "{mode:?} {range}");
                    assert!(octave < range as u32, "{mode:?} {range}");
                }
            }
        }
    }

    #[test]
    fn random_never_repeats_a_step() {
        let walk = steps(Mode::Random, 2, 4, 200);
        assert!(walk.windows(2).all(|w| w[0] != w[1]));
        assert!(walk.iter().all(|&(n, o)| n < 4 && o < 2));
    }
}
