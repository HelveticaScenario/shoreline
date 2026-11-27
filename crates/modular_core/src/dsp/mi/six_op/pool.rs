//! A lane's voices: one more than it plays at once, so a note beyond the
//! count has a silent voice to start in while the oldest note fades out.

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

    /// Starts a note on the next silent one of the first `voices` + 1 slots,
    /// fading out the oldest notes so no more than `voices` play.
    pub fn start_note(&mut self, voices: usize) {
        let voices = voices.clamp(1, MAX_VOICES);
        let in_use = voices + 1;
        let next = (1..=in_use)
            .map(|k| (self.active + k) % in_use)
            .find(|&i| self.slots[i].life == Life::Silent)
            .unwrap_or_else(|| self.oldest(in_use, |_, _| true));
        self.notes = self.notes.wrapping_add(1);
        let slot = &mut self.slots[next];
        if slot.life == Life::Silent {
            slot.voice.silence();
        } else {
            slot.voice.forget_gate();
        }
        slot.life = Life::Playing;
        slot.gain = 1.0;
        slot.started = self.notes;
        self.active = next;

        for slot in &mut self.slots[in_use..] {
            if slot.life == Life::Playing {
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

    /// One sample of every sounding voice, in slot order.
    #[inline(always)]
    pub fn render(&mut self, rates: &Rates) -> f32 {
        let mut out = 0.0;
        for slot in &mut self.slots {
            match slot.life {
                Life::Silent => {}
                Life::Playing => out += slot.voice.render(rates),
                Life::Fading => {
                    out += slot.voice.render(rates) * slot.gain;
                    slot.gain -= rates.fade;
                    if slot.gain <= 0.0 {
                        slot.life = Life::Silent;
                    }
                }
            }
        }
        out
    }
}
