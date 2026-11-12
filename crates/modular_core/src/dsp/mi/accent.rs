//! Accent as the drum machines take it: 0 V plays at Plaits' default accent,
//! 5 V plays twice as loud, and a patched trigger samples it at each strike.

use crate::dsp::drums::voice::Level;
use crate::dsp::utils::SchmittTrigger;
use crate::types::{GlideMark, SwapGlide};

/// How long a strike takes to move the level while the voice still sounds,
/// so its ringing tail doesn't step.
const RAMP_SECS: f32 = 2.0e-3;

/// Below this, in volts, the voice has fallen silent and a strike sets its
/// level at once.
const SILENT: f32 = 1.0e-3;

/// Plaits' accent knob, in its own 0–5 V, for `volts` of accent: Plaits'
/// default 4 V at no accent, its full 5 V at 5 V.
#[inline(always)]
pub(super) fn plaits_volts(volts: f32) -> f32 {
    4.0 + 0.2 * volts.clamp(0.0, 5.0)
}

/// A voice's accent and the output level it sets: 1 unaccented, rising to
/// the voice's `trim` at 5 V. The trim brings Plaits' own response to accent,
/// which already makes a hit a little louder, to twice the unaccented level.
#[derive(Clone, Copy)]
pub(super) struct Accent {
    glide: SwapGlide,
    /// The last finite accent, in volts.
    held: f32,
    trigger: SchmittTrigger,
    /// The accent the voice plays at, in volts: sampled at each strike while
    /// the trigger is patched, the live accent otherwise.
    volts: f32,
    /// Whether this sample strikes the voice.
    struck: bool,
    /// Whether the trigger is patched.
    patched: bool,
    /// The output level less one.
    level: Level,
    /// The size of the last sample in, to tell whether the voice sounds.
    last: f32,
}

impl Default for Accent {
    fn default() -> Self {
        Self {
            glide: SwapGlide::default(),
            held: 0.0,
            trigger: SchmittTrigger::default(),
            volts: 0.0,
            struck: false,
            patched: false,
            level: Level::default(),
            last: 0.0,
        }
    }
}

impl Accent {
    /// The accent to play this sample at, in volts: with `trigger` patched,
    /// the accent at its last rising edge; unpatched, the accent itself.
    #[inline(always)]
    pub(super) fn read(
        &mut self,
        (volts, mark): (f32, GlideMark),
        trigger: Option<f32>,
        sample_rate: f32,
    ) -> f32 {
        let value = self.glide.follow(volts, mark, sample_rate);
        if value.is_finite() {
            self.held = value.clamp(0.0, 5.0);
        }
        self.patched = trigger.is_some();
        self.struck = match trigger {
            Some(volts) => self.trigger.process(volts),
            None => {
                // A trigger patched again starts from low.
                self.trigger = SchmittTrigger::default();
                false
            }
        };
        if self.struck || !self.patched {
            self.volts = self.held;
        }
        self.volts
    }

    /// `out` at the level the accent [`read`](Self::read) this sample sets.
    /// A strike moves the level to its accent, ramping to it if the voice
    /// still sounds, and it holds through the hit.
    #[inline(always)]
    pub(super) fn apply(&mut self, out: f32, trim: f32, sample_rate: f32) -> f32 {
        let boost = (trim - 1.0) * self.volts * 0.2;
        if !self.patched {
            self.level.set(boost, false, 1);
        } else if self.struck {
            let samples = (RAMP_SECS * sample_rate).max(1.0) as u32;
            self.level.set(boost, self.last > SILENT, samples);
        }
        let level = 1.0 + self.level.next();
        self.last = out.abs();
        out * level
    }
}

#[cfg(test)]
mod tests {
    use super::Accent;
    use crate::dsp::mi::drum::measure::mark;

    /// A patched trigger holds the accent from its last rising edge; an
    /// unpatched one follows the accent.
    #[test]
    fn a_trigger_samples_the_accent() {
        let mut accent = Accent::default();
        let mut read =
            |volts: f32, trigger: Option<f32>| accent.read((volts, mark()), trigger, 48000.0);
        assert_eq!(read(3.0, Some(5.0)), 3.0);
        assert_eq!(read(1.0, Some(5.0)), 3.0);
        assert_eq!(read(5.0, Some(0.0)), 3.0);
        assert_eq!(read(2.0, Some(5.0)), 2.0);
        assert_eq!(read(4.0, None), 4.0);
        assert_eq!(read(1.5, None), 1.5);
    }
}
