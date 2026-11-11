//! Plaits' DX7 envelope (`fm/envelope.h`), stepped every sample. Increments
//! are per sample at 44.1 kHz, scaled to the engine rate by `scale`.

use super::patch::EnvelopeShape;

const STAGES: usize = 4;
const RELEASE: usize = STAGES - 1;
const PREVIOUS_LEVEL: f32 = -100.0;

/// Where an envelope is: its stage, its phase through it, and the level the
/// stage started from (`PREVIOUS_LEVEL`: the previous stage's level).
#[derive(Clone, Copy, Debug)]
pub struct Envelope {
    stage: usize,
    phase: f32,
    start: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            stage: RELEASE,
            phase: 1.0,
            start: 0.0,
        }
    }
}

/// The level `phase` through `stage`, from `start_level`. `RESHAPE` gives
/// rising segments the operator envelope's curve, starting no lower than 6.7.
#[inline(always)]
fn value<const RESHAPE: bool>(
    shape: &EnvelopeShape,
    stage: usize,
    mut phase: f32,
    start_level: f32,
) -> f32 {
    let mut from = if start_level == PREVIOUS_LEVEL {
        shape.level[(stage + STAGES - 1) % STAGES]
    } else {
        start_level
    };
    let mut to = shape.level[stage];
    if RESHAPE && from < to {
        from = from.max(6.7);
        to = to.max(6.7);
        phase *= (2.5 - phase) * 0.666667;
    }
    phase * (to - from) + from
}

impl Envelope {
    /// One sample at `rate` times the patch's speed, with the attack and
    /// decay stages sped up by `ad_scale` and the release by `release_scale`.
    #[inline(always)]
    pub fn render<const RESHAPE: bool>(
        &mut self,
        shape: &EnvelopeShape,
        scale: f32,
        gate: bool,
        rate: f32,
        ad_scale: f32,
        release_scale: f32,
    ) -> f32 {
        if gate {
            if self.stage == RELEASE {
                self.start = value::<RESHAPE>(shape, self.stage, self.phase, self.start);
                self.stage = 0;
                self.phase = 0.0;
            }
        } else if self.stage != RELEASE {
            self.start = value::<RESHAPE>(shape, self.stage, self.phase, self.start);
            self.stage = RELEASE;
            self.phase = 0.0;
        }
        let stage_scale = if self.stage == RELEASE {
            release_scale
        } else {
            ad_scale
        };
        self.phase += shape.increment[self.stage] * scale * rate * stage_scale;
        if self.phase >= 1.0 {
            if self.stage >= STAGES - 2 {
                self.phase = 1.0;
            } else {
                self.phase = 0.0;
                self.stage += 1;
            }
            self.start = PREVIOUS_LEVEL;
        }
        value::<RESHAPE>(shape, self.stage, self.phase, self.start)
    }
}

/// The level `t` samples after a note-on, for a gate held `gate_duration`
/// samples: the envelope scrubbed to a point rather than run.
pub fn render_at_sample<const RESHAPE: bool>(
    shape: &EnvelopeShape,
    scale: f32,
    mut t: f32,
    gate_duration: f32,
) -> f32 {
    let increment = |stage: usize| shape.increment[stage] * scale;
    if t > gate_duration {
        let phase = (t - gate_duration) * increment(RELEASE);
        return if phase >= 1.0 {
            shape.level[RELEASE]
        } else {
            let start = render_at_sample::<RESHAPE>(shape, scale, gate_duration, gate_duration);
            value::<RESHAPE>(shape, RELEASE, phase, start)
        };
    }
    let mut stage = 0;
    while stage < RELEASE {
        let stage_duration = 1.0 / increment(stage);
        if t < stage_duration {
            break;
        }
        t -= stage_duration;
        stage += 1;
    }
    if stage == RELEASE {
        t -= gate_duration;
        if t <= 0.0 {
            return shape.level[RELEASE - 1];
        } else if t * increment(RELEASE) > 1.0 {
            return shape.level[RELEASE];
        }
    }
    value::<RESHAPE>(shape, stage, t * increment(stage), PREVIOUS_LEVEL)
}
