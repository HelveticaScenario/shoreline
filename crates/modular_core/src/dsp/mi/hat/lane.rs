//! A lane of `$mi.hiHat`: the strike and knobs its two hats share, and each
//! hat with its own noise.

use crate::dsp::{
    mi::{
        accent::{Accent, AccentLevel, plaits_volts},
        core::random::Random,
        drum::{Knob, OUTPUT_GAIN, rises},
    },
    utils::{SchmittTrigger, rng::hash_bipolar},
};
use crate::types::GlideMark;

use super::hat_drum::{HatKnobs, HatRates, HiHat, Model};

/// The knobs' defaults in volts, which each module's params declare.
pub(super) const KNOB_DEFAULT: f32 = 2.5;
pub(super) const ACCENT_DEFAULT: f32 = 0.0;

/// The output level at full accent, which with Plaits' own response to
/// accent makes a fully accented hit peak twice as high as an unaccented one.
pub(super) const ACCENT_TRIM: f32 = 1.72;

pub(super) struct HatState {
    pub(super) sample_rate: f32,
    pub(super) inv_sample_rate: f32,
    rates: HatRates,
}

impl Default for HatState {
    fn default() -> Self {
        Self::new(48000.0)
    }
}

impl HatState {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            rates: HatRates::new(sample_rate),
        }
    }
}

/// The knob inputs for a sample, in volts, with their glide marks.
#[derive(Clone, Copy)]
pub(super) struct Knobs {
    pub tone: (f32, GlideMark),
    pub decay: (f32, GlideMark),
    pub noisiness: (f32, GlideMark),
    pub accent: (f32, GlideMark),
}

/// What both of a lane's hats read alike each sample: the trigger, the
/// knobs and the accent.
#[derive(Clone, Copy, SeedSide)]
pub(super) struct Strike {
    trigger: SchmittTrigger,
    tone: Knob,
    decay: Knob,
    noisiness: Knob,
    accent: Knob,
    accent_state: Accent,
}

impl Default for Strike {
    fn default() -> Self {
        Self {
            trigger: SchmittTrigger::default(),
            tone: Knob::new(KNOB_DEFAULT),
            decay: Knob::new(KNOB_DEFAULT),
            noisiness: Knob::new(KNOB_DEFAULT),
            accent: Knob::new(plaits_volts(ACCENT_DEFAULT)),
            accent_state: Accent::default(),
        }
    }
}

/// A strike as the hats play it, from [`Strike::read`].
pub(super) struct Struck<'a> {
    /// The trigger is unpatched, so the hats drone.
    sustain: bool,
    /// The trigger rose this sample.
    triggered: bool,
    knobs: HatKnobs,
    accent: &'a Accent,
}

impl Strike {
    /// The strike for a sample, its accent read from `knobs` at each rising
    /// `trigger`; `None` is an unpatched trigger.
    #[inline(always)]
    pub(super) fn read(
        &mut self,
        knobs: Knobs,
        trigger: Option<f32>,
        state: &HatState,
    ) -> Struck<'_> {
        let accent = self
            .accent_state
            .read(knobs.accent, trigger, state.sample_rate);
        let knobs = Knobs {
            accent: (plaits_volts(accent), knobs.accent.1),
            ..knobs
        };
        self.read_plaits(knobs, trigger, state)
    }

    /// [`read`](Self::read) with `knobs.accent` already in Plaits' volts.
    #[inline(always)]
    pub(super) fn read_plaits(
        &mut self,
        knobs: Knobs,
        trigger: Option<f32>,
        state: &HatState,
    ) -> Struck<'_> {
        let sample_rate = state.sample_rate;
        Struck {
            sustain: trigger.is_none(),
            triggered: rises(&mut self.trigger, trigger),
            knobs: HatKnobs {
                accent: self.accent.read(knobs.accent, sample_rate),
                tone: self.tone.read(knobs.tone, sample_rate),
                decay: self.decay.read(knobs.decay, sample_rate),
                noisiness: self.noisiness.read(knobs.noisiness, sample_rate),
            },
            accent: &self.accent_state,
        }
    }
}

/// One hat of a lane.
#[derive(Clone, Copy, SeedSide)]
pub(super) struct Lane<M: Model> {
    hat: HiHat<M>,
    /// The lane's clocked noise.
    #[seed_side(keep)]
    random: Random,
    level: AccentLevel,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// lane, so its noise stays continuous across patch updates.
    #[seed_side(keep)]
    seeded: bool,
}

impl<M: Model> Default for Lane<M> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<M: Model> Lane<M> {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            hat: HiHat::default(),
            random: Random::new(seed),
            level: AccentLevel::default(),
            seeded: false,
        }
    }

    /// One sample of the hat's output at `f0` cycles per sample.
    #[inline(always)]
    pub(super) fn play(&mut self, f0: f32, struck: &Struck, state: &HatState) -> f32 {
        let out = (OUTPUT_GAIN * self.step(f0, struck, state)).clamp(-5.0, 5.0);
        struck
            .accent
            .apply_to(&mut self.level, out, ACCENT_TRIM, state.sample_rate)
    }

    /// One sample at `f0` cycles per sample, before the output gain and the
    /// accent's level.
    #[inline(always)]
    pub(super) fn step(&mut self, f0: f32, struck: &Struck, state: &HatState) -> f32 {
        self.hat.render(
            struck.sustain,
            struck.triggered,
            f0,
            &struck.knobs,
            &mut self.random,
            &state.rates,
        )
    }
}

/// Seeds the noise generators of `lanes` that have none yet, from `base`,
/// the module's [`seed_base`](crate::dsp::utils::rng::seed_base), and
/// `stream`, which gives each of a module's hats noise of its own. Runs in
/// `on_patch_update`, not `init`, because `init` runs in a stack slot that
/// consecutive constructions share, so every instance would draw the same
/// noise.
pub(super) fn seed<'a, M: Model + 'a>(
    base: u64,
    stream: u64,
    lanes: impl Iterator<Item = &'a mut Lane<M>>,
) {
    for (lane_index, lane) in lanes.enumerate() {
        if !lane.seeded {
            let index = lane_index as u64 | stream << 32;
            let unit = (hash_bipolar(base, index) + 1.0) * 0.5;
            lane.random = Random::new((unit * f64::from(u32::MAX)) as u32);
            lane.seeded = true;
        }
    }
}
