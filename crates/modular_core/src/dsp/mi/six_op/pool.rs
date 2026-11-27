//! A lane's voices: one more than it plays at once, so a note beyond the
//! count has a silent voice to start in while the oldest note fades out.

use super::super::core::dsp::soft_clip;
use super::voice::{FmVoice, Rates};

/// The most voices a lane plays at once: the top of `voices`' range.
pub const MAX_VOICES: usize = 8;
const SLOTS: usize = MAX_VOICES + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Life {
    /// Faded out or never played; not rendered.
    Silent,
    Playing,
    /// Fading out at `gain`.
    Fading,
}

#[derive(Clone, Copy)]
struct Slot {
    voice: FmVoice,
    life: Life,
    gain: f32,
    /// The output level its note's accent set.
    level: f32,
    /// The note count when its note started, to find the oldest.
    started: u32,
}

#[derive(Clone, Copy)]
pub struct Pool {
    slots: [Slot; SLOTS],
    /// The slot the gate plays.
    active: usize,
    notes: u32,
}

impl Default for Pool {
    fn default() -> Self {
        Self {
            slots: [Slot {
                voice: FmVoice::default(),
                life: Life::Silent,
                gain: 0.0,
                level: 1.0,
                started: 0,
            }; SLOTS],
            active: SLOTS - 1,
            notes: 0,
        }
    }
}

impl Pool {
    pub fn active(&mut self) -> &mut FmVoice {
        &mut self.slots[self.active].voice
    }

    /// Starts a note at `level` on the next silent one of the first
    /// `voices` + 1 slots, or on any silent slot when those all sound,
    /// fading out the oldest notes so no more than `voices` play.
    pub fn start_note(&mut self, voices: usize, level: f32) {
        let voices = voices.clamp(1, MAX_VOICES);
        let in_use = voices + 1;
        let silent = |i: &usize| self.slots[*i].life == Life::Silent;
        let next = (1..=in_use)
            .map(|k| (self.active + k) % in_use)
            .find(silent)
            .or_else(|| (in_use..SLOTS).find(silent))
            .unwrap_or_else(|| self.oldest(in_use, |_, _| true));
        self.notes = self.notes.wrapping_add(1);
        let slot = &mut self.slots[next];
        if slot.life == Life::Silent {
            slot.voice.silence();
        }
        slot.life = Life::Playing;
        slot.gain = 1.0;
        slot.level = level;
        slot.started = self.notes;
        self.active = next;

        for (i, slot) in self.slots.iter_mut().enumerate().skip(in_use) {
            if i != next && slot.life == Life::Playing {
                slot.life = Life::Fading;
            }
        }
        let others = |pool: &Self| {
            pool.slots[..in_use]
                .iter()
                .enumerate()
                .filter(|&(i, s)| i != next && s.life == Life::Playing)
                .count()
        };
        while others(self) >= voices {
            let oldest = self.oldest(in_use, |i, s| i != next && s.life == Life::Playing);
            self.slots[oldest].life = Life::Fading;
        }
    }

    /// Sets the active note's level.
    pub fn set_level(&mut self, level: f32) {
        self.slots[self.active].level = level;
    }

    /// The slot among the first `in_use` that `keep` accepts whose note
    /// started longest ago.
    fn oldest(&self, in_use: usize, keep: impl Fn(usize, &Slot) -> bool) -> usize {
        self.slots[..in_use]
            .iter()
            .enumerate()
            .filter(|&(i, s)| keep(i, s))
            .max_by_key(|&(i, s)| (self.notes.wrapping_sub(s.started), usize::MAX - i))
            .map_or(0, |(i, _)| i)
    }

    /// Each sounding voice but the active one, in slot order.
    pub fn for_each_other(&mut self, mut f: impl FnMut(&mut FmVoice)) {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if i != self.active && slot.life != Life::Silent {
                f(&mut slot.voice);
            }
        }
    }

    /// One sample of every sounding voice, each through Plaits' output stage
    /// at its level, summed in slot order.
    #[inline(always)]
    pub fn render(&mut self, rates: &Rates) -> f32 {
        let mut out = 0.0;
        for slot in &mut self.slots {
            if slot.life == Life::Silent {
                continue;
            }
            let sample = soft_clip(slot.voice.render(rates) * 0.25) * slot.level;
            if slot.life == Life::Playing {
                out += sample;
            } else {
                out += sample * slot.gain;
                slot.gain -= rates.fade;
                if slot.gain <= 0.0 {
                    slot.life = Life::Silent;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lowering `voices` while notes ring starts the next note on a silent
    /// slot, past the new count when none below it is, fading every note
    /// it leaves over the count.
    #[test]
    fn lowering_the_count_never_reuses_a_sounding_voice() {
        let mut pool = Pool::default();
        for _ in 0..4 {
            pool.start_note(3, 1.0);
        }
        let playing: Vec<usize> = (0..SLOTS)
            .filter(|&i| pool.slots[i].life == Life::Playing)
            .collect();
        assert_eq!(playing.len(), 3);
        assert!((0..2).all(|i| pool.slots[i].life != Life::Silent));
        pool.start_note(1, 1.0);
        assert!(!playing.contains(&pool.active));
        assert_eq!(pool.slots[pool.active].life, Life::Playing);
        for i in playing {
            assert_eq!(pool.slots[i].life, Life::Fading, "slot {i}");
        }
    }
}
