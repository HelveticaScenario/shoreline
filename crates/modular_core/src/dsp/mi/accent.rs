//! Accent as the drum machines take it: 0 V plays at Plaits' default accent,
//! and 5 V plays twice as loud.

use crate::dsp::utils::SchmittTrigger;
use crate::types::{GlideMark, SwapGlide};

/// Plaits' accent knob, in its own 0–5 V, for `volts` of accent: Plaits'
/// default 4 V at no accent, its full 5 V at 5 V. NaN passes through, for the
/// knob to hold its last value.
#[inline(always)]
pub(super) fn plaits_volts(volts: f32) -> f32 {
    4.0 + 0.2 * volts.clamp(0.0, 5.0)
}

/// The output level an accent sets: 1 unaccented, rising to a voice's `trim`
/// at 5 V. The trim brings Plaits' own response to accent, which already
/// makes a hit a little louder, to twice the unaccented level.
#[derive(Clone, Copy)]
pub(super) struct AccentLevel {
    glide: SwapGlide,
    /// The last finite accent, in volts.
    held: f32,
    trigger: SchmittTrigger,
    level: f32,
}

impl Default for AccentLevel {
    fn default() -> Self {
        Self {
            glide: SwapGlide::default(),
            held: 0.0,
            trigger: SchmittTrigger::default(),
            level: 1.0,
        }
    }
}

impl AccentLevel {
    /// The level for this sample. With `trigger` patched, a rising edge sets
    /// it from the accent then, and it holds through the hit; unpatched, it
    /// follows the accent.
    #[inline(always)]
    pub(super) fn next(
        &mut self,
        (volts, mark): (f32, GlideMark),
        trigger: Option<f32>,
        trim: f32,
        sample_rate: f32,
    ) -> f32 {
        let value = self.glide.follow(volts, mark, sample_rate);
        if value.is_finite() {
            self.held = value;
        }
        let struck = match trigger {
            Some(volts) => self.trigger.process(volts),
            None => {
                // A trigger patched again starts from low.
                self.trigger = SchmittTrigger::default();
                true
            }
        };
        if struck {
            self.level = 1.0 + (trim - 1.0) * self.held.clamp(0.0, 5.0) * 0.2;
        }
        self.level
    }
}
