//! What `$mi.hiHat` and `$mi.ringHat` share: a channel's hat, noise,
//! trigger and knobs.

use crate::dsp::{
    mi::{
        core::random::Random,
        drum::{Knob, rises},
    },
    utils::{SchmittTrigger, rng::hash_bipolar},
};
use crate::types::GlideMark;

use super::hat_drum::{HatKnobs, HatRates, HiHat, Model};

/// The knobs' defaults in volts, which each module's params declare.
pub(super) const KNOB_DEFAULT: f32 = 2.5;
pub(super) const ACCENT_DEFAULT: f32 = 4.0;

pub(super) struct HatState {
    sample_rate: f32,
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
pub(super) struct Knobs {
    pub tone: (f32, GlideMark),
    pub decay: (f32, GlideMark),
    pub noisiness: (f32, GlideMark),
    pub accent: (f32, GlideMark),
}

pub(super) struct Channel<M: Model> {
    hat: HiHat<M>,
    /// The channel's clocked noise.
    random: Random,
    trigger: SchmittTrigger,
    tone: Knob,
    decay: Knob,
    noisiness: Knob,
    accent: Knob,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// channel, so its noise stays continuous across patch updates.
    seeded: bool,
}

impl<M: Model> Default for Channel<M> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<M: Model> Channel<M> {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            hat: HiHat::default(),
            random: Random::new(seed),
            trigger: SchmittTrigger::default(),
            tone: Knob::new(KNOB_DEFAULT),
            decay: Knob::new(KNOB_DEFAULT),
            noisiness: Knob::new(KNOB_DEFAULT),
            accent: Knob::new(ACCENT_DEFAULT),
            seeded: false,
        }
    }

    /// One sample at `f0` cycles per sample, before the output gain. A rising
    /// `trigger` strikes the hat; `None` is an unpatched trigger.
    #[inline(always)]
    pub(super) fn step(
        &mut self,
        f0: f32,
        knobs: Knobs,
        trigger: Option<f32>,
        state: &HatState,
    ) -> f32 {
        let triggered = rises(&mut self.trigger, trigger);
        let sample_rate = state.sample_rate;
        let hat_knobs = HatKnobs {
            accent: self.accent.read(knobs.accent, sample_rate),
            tone: self.tone.read(knobs.tone, sample_rate),
            decay: self.decay.read(knobs.decay, sample_rate),
            noisiness: self.noisiness.read(knobs.noisiness, sample_rate),
        };
        self.hat.render(
            trigger.is_none(),
            triggered,
            f0,
            &hat_knobs,
            &mut self.random,
            &state.rates,
        )
    }
}

/// Seeds the noise generators of `channels` that have none yet, from `base`,
/// the module's [`seed_base`](crate::dsp::utils::rng::seed_base). Runs in
/// `on_patch_update`, not `init`, because `init` runs in a stack slot that
/// consecutive constructions share, so every instance would draw the same
/// noise.
pub(super) fn seed<M: Model>(base: u64, channels: &mut [Channel<M>]) {
    for (ch, channel) in channels.iter_mut().enumerate() {
        if !channel.seeded {
            let unit = (hash_bipolar(base, ch as u64) + 1.0) * 0.5;
            channel.random = Random::new((unit * f64::from(u32::MAX)) as u32);
            channel.seeded = true;
        }
    }
}
