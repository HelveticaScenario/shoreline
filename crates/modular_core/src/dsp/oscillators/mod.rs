use std::collections::HashMap;

use deserr::Deserr;
use schemars::JsonSchema;
use serde::Serialize;

use crate::block_input::BlockInput;
use crate::dsp::utils::{GATE_DETECTION_HIGH_THRESHOLD, SchmittTrigger, voct_to_hz, wrap_phase};
use crate::params::ParamsDeserializer;
use crate::poly::PORT_MAX_LANES;
use crate::types::{Module, ModuleSchema, SampleableConstructor};

/// FM synthesis mode for oscillators.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, Serialize, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum FmMode {
    /// through-zero: deep modulation can push the frequency below zero, running the wave backwards
    #[default]
    ThroughZero,
    /// linear: like throughZero, but the frequency stops at zero
    Lin,
    /// exponential: `fm` adds to the pitch, one octave per volt
    Exp,
}

/// Calculate frequency with FM modulation applied.
///
/// Given a base pitch in V/Oct, an FM modulation value, and an FM mode,
/// returns the modulated frequency in Hz.
#[inline]
pub fn apply_fm(pitch: f32, fm: f32, fm_mode: FmMode) -> f32 {
    match fm_mode {
        FmMode::Exp => voct_to_hz(pitch + fm),
        FmMode::Lin => (voct_to_hz(pitch) * (1.0 + fm)).max(0.0),
        FmMode::ThroughZero => voct_to_hz(pitch) * (1.0 + fm),
    }
}

/// Subsample position in `[0, 1)` at which a rising sync edge crossed the gate
/// high threshold, linearly interpolated between the previous and current sync
/// samples. Used to place the [`sync_blep`] correction within the sample.
#[inline]
pub fn sync_edge_fraction(prev: f32, curr: f32) -> f32 {
    let denom = curr - prev;
    if denom > 1.0e-12 {
        ((GATE_DETECTION_HIGH_THRESHOLD - prev) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Two-point PolyBLEP correction for a hard-sync phase reset.
///
/// The reset is a step of size `jump` (the naive output *after* the reset minus
/// the value *before* it) located `frac` of a sample into the upcoming sample
/// interval. Returns `(this_sample, next_sample)`: residuals to add to the
/// naive output now and to carry into the following sample. Calibrated to match
/// the ±1-normalized PolyBLEP used for the oscillators' natural edges, so the
/// synced sample lands on the band-limited midpoint when `frac == 0`.
#[inline]
pub fn sync_blep(jump: f32, frac: f32) -> (f32, f32) {
    let half = 0.5 * jump;
    let before = 1.0 - frac;
    (half * before * before, -half * frac * frac)
}

/// How many times faster a hard-synced oscillator runs than its note,
/// `octaves` above it.
#[inline(always)]
pub fn sync_ratio(octaves: f32) -> f32 {
    octaves.max(0.0).exp2()
}

/// The fastest a synced oscillator runs, in cycles per sample. At Nyquist its
/// band-limiting corrections still keep it within its range.
pub const MAX_SYNCED_INCREMENT: f32 = 0.5;

/// A restart of an oscillator's cycle, landing `frac` of a sample into the
/// step its oscillator's [`Landing`] names. The cycle starts over exactly
/// there, so a restart at audio rate keeps its pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reset {
    /// A rising edge on `restart`.
    Restart(f32),
    /// The sync source completed a cycle.
    Sync(f32),
}

impl Reset {
    pub fn frac(self) -> f32 {
        match self {
            Reset::Restart(frac) | Reset::Sync(frac) => frac,
        }
    }

    /// A restart where one lands, otherwise a sync reset where one lands.
    #[inline(always)]
    pub fn first(restarted: Option<f32>, wrapped: Option<f32>) -> Option<Reset> {
        restarted.map(Reset::Restart).or(wrapped.map(Reset::Sync))
    }

    /// The phase to store so that the next step of `dt` lands where the
    /// restarted cycle stands at its end, the cycle starting `frac` into the
    /// step.
    #[inline(always)]
    pub fn phase(self, dt: f32) -> f32 {
        wrap_phase(-self.frac() * dt)
    }
}

/// Which step an oscillator's resets land in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landing {
    /// The step being taken: the oscillator band-limits a reset within it.
    Now,
    /// The step after: the oscillator band-limits a reset across the samples
    /// either side of it, so its sync source looks one step ahead.
    Next,
}

impl Landing {
    /// When a sync source whose phase mirrors the oscillator's at `before`,
    /// moving by `moved` a step, completes a cycle in the step a reset lands
    /// in, how far into that step it does.
    #[inline(always)]
    pub fn wrap(self, before: f32, moved: f32) -> Option<f32> {
        let start = match self {
            Landing::Now => before,
            Landing::Next => wrap_phase(before + moved),
        };
        wrap_fraction(start, moved)
    }

    /// The sync source's phase once a reset `frac` into the landing step
    /// restarts its cycle, mirroring the oscillator's.
    #[inline(always)]
    pub fn restarted(self, frac: f32, dt: f32) -> f32 {
        match self {
            Landing::Now => wrap_phase((1.0 - frac) * dt),
            Landing::Next => wrap_phase(-frac * dt),
        }
    }
}

/// A synced oscillator's increment, `ratio` times its note's `dt`.
#[inline(always)]
pub fn synced_increment(dt: f32, ratio: f32) -> f32 {
    (dt * ratio).clamp(-MAX_SYNCED_INCREMENT, MAX_SYNCED_INCREMENT)
}

/// When a phase standing at `before` moves by `moved` across the end of its
/// cycle, how far between the last sample and this one it crossed.
#[inline(always)]
pub fn wrap_fraction(before: f32, moved: f32) -> Option<f32> {
    let after = before + moved;
    if after >= 1.0 {
        Some(((1.0 - before) / moved).clamp(0.0, 1.0))
    } else if after < 0.0 {
        Some((before / -moved).clamp(0.0, 1.0))
    } else {
        None
    }
}

/// Rising edges on a `restart` input, each placed at its subsample crossing.
#[derive(Default, Clone, Copy)]
pub struct RestartEdge {
    schmitt: SchmittTrigger,
    /// The previous input sample, for subsample edge interpolation.
    prev: f32,
}

impl RestartEdge {
    /// Track one input sample; on a rising edge, returns how far between the
    /// last sample and this one it crossed.
    #[inline(always)]
    pub fn process(&mut self, v: f32) -> Option<f32> {
        let edge = self
            .schmitt
            .process(v)
            .then(|| sync_edge_fraction(self.prev, v));
        self.prev = v;
        edge
    }
}

/// Each lane's sync ratio through a block, computed once for a `sync` that
/// holds still.
pub struct SyncRatios<'a> {
    sync: Option<BlockInput<'a>>,
    ratios: [Option<f32>; PORT_MAX_LANES],
}

impl<'a> SyncRatios<'a> {
    pub fn new(sync: Option<BlockInput<'a>>, start: usize) -> Self {
        let mut ratios = Self {
            sync,
            ratios: [None; PORT_MAX_LANES],
        };
        ratios.fill(start);
        ratios
    }

    /// Slot `i`'s ratio in each lane, `None` everywhere while `sync` is
    /// disconnected.
    #[inline(always)]
    pub fn at(&mut self, i: usize) -> &[Option<f32>; PORT_MAX_LANES] {
        if self.sync.is_some_and(|sync| !sync.is_constant()) {
            self.fill(i);
        }
        &self.ratios
    }

    #[inline(always)]
    fn fill(&mut self, i: usize) {
        if let Some(sync) = self.sync {
            for (ratio, &octaves) in self.ratios.iter_mut().zip(sync.row(i)) {
                *ratio = Some(sync_ratio(octaves));
            }
        }
    }
}

/// One lane's `restart` edge detector and `sync` source: a cycle at the
/// note's pitch that restarts the oscillator each time it completes, while
/// the oscillator runs `sync_ratio` times faster.
#[derive(Default, Clone, Copy)]
pub struct Resets {
    restart: RestartEdge,
    /// The sync source's phase, a cycle at the note's pitch. It mirrors the
    /// phase the oscillator stores, and unsynced moves with it, so turning
    /// sync on moves nothing.
    source: f32,
}

impl Resets {
    /// Start the sync source on the oscillator's own starting phase.
    pub fn seed(&mut self, phase: f32) {
        self.source = phase;
    }

    /// Advance one sample of an oscillator whose stored phase stands at
    /// `phase` and whose note runs `dt` cycles per sample. Returns the
    /// oscillator's own increment and any reset landing in the step `landing`
    /// names. A restart also restarts the sync source, and wins when both
    /// land together.
    #[inline(always)]
    pub fn step(
        &mut self,
        phase: f32,
        dt: f32,
        restart: Option<f32>,
        sync_ratio: Option<f32>,
        landing: Landing,
    ) -> (f32, Option<Reset>) {
        let restarted = restart.and_then(|v| self.restart.process(v));
        let (increment, wrapped) = match sync_ratio {
            Some(ratio) => {
                let wrapped = landing.wrap(self.source, dt);
                (synced_increment(dt, ratio), wrapped)
            }
            None => (dt, None),
        };
        self.source = match (restarted, sync_ratio) {
            (Some(frac), _) => landing.restarted(frac, dt),
            (None, Some(_)) => wrap_phase(self.source + dt),
            (None, None) => wrap_phase(phase + dt),
        };
        (increment, Reset::first(restarted, wrapped))
    }
}

/// Two-point PolyBLAMP correction for a reset's change of slope:
/// `slope_change` is the slope after the reset minus the slope before, in
/// output units per sample, located `frac` of a sample into the upcoming
/// interval. Returns `(this_sample, next_sample)` like [`sync_blep`], whose
/// residual this one integrates.
#[inline]
pub fn sync_blamp(slope_change: f32, frac: f32) -> (f32, f32) {
    let sixth = slope_change / 6.0;
    let before = 1.0 - frac;
    (sixth * before * before * before, sixth * frac * frac * frac)
}

pub mod enosc;
pub mod noise;
pub mod p_pulse;
pub mod p_saw;
pub mod p_sine;
pub mod pulse;
pub mod saw;
pub mod sine;
pub mod supersaw;
pub mod wavetable;
pub mod wavetable_prep;

pub fn install_constructors(map: &mut HashMap<String, SampleableConstructor>) {
    sine::SineOscillator::install_constructor(map);
    saw::SawOscillator::install_constructor(map);
    pulse::PulseOscillator::install_constructor(map);
    p_sine::PSineOscillator::install_constructor(map);
    p_saw::PSawOscillator::install_constructor(map);
    p_pulse::PPulseOscillator::install_constructor(map);
    noise::Noise::install_constructor(map);
    supersaw::Supersaw::install_constructor(map);
    enosc::Enosc::install_constructor(map);
    wavetable::WavetableOsc::install_constructor(map);
}

pub fn install_params_deserializers(map: &mut HashMap<String, ParamsDeserializer>) {
    sine::SineOscillator::install_params_deserializer(map);
    saw::SawOscillator::install_params_deserializer(map);
    pulse::PulseOscillator::install_params_deserializer(map);
    p_sine::PSineOscillator::install_params_deserializer(map);
    p_saw::PSawOscillator::install_params_deserializer(map);
    p_pulse::PPulseOscillator::install_params_deserializer(map);
    noise::Noise::install_params_deserializer(map);
    supersaw::Supersaw::install_params_deserializer(map);
    enosc::Enosc::install_params_deserializer(map);
    wavetable::WavetableOsc::install_params_deserializer(map);
}

pub fn schemas() -> Vec<ModuleSchema> {
    vec![
        sine::SineOscillator::get_schema(),
        saw::SawOscillator::get_schema(),
        pulse::PulseOscillator::get_schema(),
        p_sine::PSineOscillator::get_schema(),
        p_saw::PSawOscillator::get_schema(),
        p_pulse::PPulseOscillator::get_schema(),
        noise::Noise::get_schema(),
        supersaw::Supersaw::get_schema(),
        enosc::Enosc::get_schema(),
        wavetable::WavetableOsc::get_schema(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsynced_resets_only_restart_on_rising_edges() {
        let mut resets = Resets::default();
        assert_eq!(
            resets.step(0.3, 0.01, Some(0.0), None, Landing::Next),
            (0.01, None)
        );
        assert_eq!(
            resets.step(0.31, 0.01, Some(5.0), None, Landing::Next),
            (0.01, Some(Reset::Restart(0.2)))
        );
        assert_eq!(
            resets.step(0.0, 0.01, Some(5.0), None, Landing::Next),
            (0.01, None)
        );
    }

    #[test]
    fn sync_runs_faster_and_resets_once_per_note_cycle() {
        let mut resets = Resets::default();
        let dt = 0.01;
        let mut fracs = Vec::new();
        for _ in 0..1005 {
            let (increment, reset) =
                resets.step(0.0, dt, None, Some(sync_ratio(1.5)), Landing::Now);
            assert!((increment - dt * 1.5f32.exp2()).abs() < 1e-7);
            fracs.extend(reset.map(Reset::frac));
        }
        assert_eq!(fracs.len(), 10);
        assert!(fracs.iter().all(|frac| (0.0..=1.0).contains(frac)));
    }

    #[test]
    fn a_restart_wins_and_restarts_the_sync_source() {
        let mut resets = Resets::default();
        resets.seed(0.995);
        let (_, reset) = resets.step(0.0, 0.01, Some(5.0), Some(1.0), Landing::Now);
        assert_eq!(reset, Some(Reset::Restart(0.2)));
        // The source starts over with the cycle, a whole note cycle from
        // its next completion.
        let resets_after = (0..99)
            .filter_map(|_| resets.step(0.0, 0.01, Some(5.0), Some(1.0), Landing::Now).1)
            .count();
        assert_eq!(resets_after, 0);
    }

    #[test]
    fn sync_edge_fraction_interpolates_threshold_crossing() {
        // Crossing 1.0V on the way up from 0 to 5 happens 1/5 of the way in.
        assert!((sync_edge_fraction(0.0, 5.0) - 0.2).abs() < 1e-6);
        // Midway crossing.
        assert!((sync_edge_fraction(0.5, 1.5) - 0.5).abs() < 1e-6);
        // Already above threshold at the start → crossing at the very start.
        assert_eq!(sync_edge_fraction(1.0, 5.0), 0.0);
        // Non-rising / degenerate inputs fall back to 0.
        assert_eq!(sync_edge_fraction(2.0, 1.0), 0.0);
        assert_eq!(sync_edge_fraction(3.0, 3.0), 0.0);
    }

    #[test]
    fn sync_blamp_integrates_the_blep_residual() {
        // A corner exactly on a sample bends that sample by a sixth of the
        // slope change and leaves the next untouched; at frac 1 the roles
        // swap. Halfway, both samples share the bend.
        assert_eq!(sync_blamp(6.0, 0.0), (1.0, 0.0));
        assert_eq!(sync_blamp(6.0, 1.0), (0.0, 1.0));
        assert_eq!(sync_blamp(6.0, 0.5), (0.125, 0.125));
    }

    #[test]
    fn sync_blep_lands_the_discontinuity_sample_on_the_midpoint() {
        let m = 2.0;
        // frac == 0: the whole correction is on this sample → before + M/2.
        let (now, carry) = sync_blep(m, 0.0);
        assert!((now - 1.0).abs() < 1e-6);
        assert!(carry.abs() < 1e-6);
        // frac → 1: nothing now, the next sample carries −M/2 → after − M/2.
        let (now, carry) = sync_blep(m, 1.0);
        assert!(now.abs() < 1e-6);
        assert!((carry + 1.0).abs() < 1e-6);
        // A zero jump produces no correction at all.
        assert_eq!(sync_blep(0.0, 0.3), (0.0, 0.0));
    }
}
