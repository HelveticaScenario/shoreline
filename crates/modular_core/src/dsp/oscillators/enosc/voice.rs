//! One `$enosc` voice: a pair of sine oscillators on adjacent scale notes,
//! crossfaded by where the voice's root sits between them.
//!
//! Adapted from the 4ms Ensemble Oscillator. Copyright 4ms Company. Used under
//! GPL v3.

use deserr::Deserr;
use schemars::JsonSchema;
use serde::Serialize;

use crate::dsp::fx::enosc_tables::{
    aa_cheby, aa_fold, aa_segment, fold_amount, interpolate_cheby, interpolate_segment, lookup_fold,
};
use crate::dsp::phase_warp::{PhaseWarp, PhaseWarpState, ResolvedWarp};
use crate::dsp::utils::fade::FadeIn;
use crate::dsp::utils::{sin_turns, voct_to_hz};

use super::scale::{PitchPair, ScaleNotes};

/// The order the fold (`f`), cheby (`c`) and segment (`s`) warps run in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, Serialize, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum EffectOrder {
    #[default]
    Fcs,
    Fsc,
    Cfs,
    Csf,
    Sfc,
    Scf,
}

const FOLD: u8 = 1;
const CHEBY: u8 = 2;
const SEGMENT: u8 = 3;

impl EffectOrder {
    fn stages(self) -> [u8; 3] {
        match self {
            EffectOrder::Fcs => [FOLD, CHEBY, SEGMENT],
            EffectOrder::Fsc => [FOLD, SEGMENT, CHEBY],
            EffectOrder::Cfs => [CHEBY, FOLD, SEGMENT],
            EffectOrder::Csf => [CHEBY, SEGMENT, FOLD],
            EffectOrder::Sfc => [SEGMENT, FOLD, CHEBY],
            EffectOrder::Scf => [SEGMENT, CHEBY, FOLD],
        }
    }
}

type ChainFn = fn(&WarpChain, f32, f32) -> f32;

/// The fold/cheby/segment chain applied to every oscillator's sine. Each amount
/// is a 0–1 knob; a stage at 0 is the identity and is skipped.
#[derive(Clone, Copy, Default)]
pub(super) struct WarpChain {
    /// The chain compiled for exactly the active stages in order; `None` when
    /// every stage is off.
    run: Option<ChainFn>,
    order: EffectOrder,
    fold: f32,
    cheby: f32,
    segment: f32,
}

impl WarpChain {
    /// Recompile the chain if any setting differs from the one it was built for.
    #[inline]
    pub fn set(&mut self, order: EffectOrder, fold: f32, cheby: f32, segment: f32) {
        if order != self.order
            || fold != self.fold
            || cheby != self.cheby
            || segment != self.segment
        {
            *self = Self::new(order, fold, cheby, segment);
        }
    }

    fn new(order: EffectOrder, fold: f32, cheby: f32, segment: f32) -> Self {
        let amount = |stage| match stage {
            FOLD => fold,
            CHEBY => cheby,
            _ => segment,
        };
        let mut active = [0u8; 3];
        let mut len = 0;
        for stage in order.stages() {
            if amount(stage) > 0.0 {
                active[len] = stage;
                len += 1;
            }
        }
        Self {
            run: select(active),
            order,
            fold,
            cheby,
            segment,
        }
    }

    #[inline]
    pub fn active(&self) -> bool {
        self.run.is_some()
    }

    #[inline]
    fn apply(&self, x: f32, freq_norm: f32) -> f32 {
        match self.run {
            Some(run) => run(self, x, freq_norm),
            None => x,
        }
    }

    #[inline(always)]
    fn stage<const K: u8>(&self, x: f32, freq_norm: f32) -> f32 {
        match K {
            FOLD => lookup_fold(x, aa_fold(freq_norm, fold_amount(self.fold))),
            CHEBY => interpolate_cheby(x, aa_cheby(freq_norm, self.cheby)),
            SEGMENT => interpolate_segment(x, aa_segment(freq_norm, self.segment)),
            _ => x,
        }
    }
}

// Kept out of line: inlined, the table lookups bloat the oscillator loop and
// slow it down even while every stage is off.
#[inline(never)]
fn chain<const A: u8, const B: u8, const C: u8>(warps: &WarpChain, x: f32, f: f32) -> f32 {
    let x = warps.stage::<A>(x, f);
    let x = warps.stage::<B>(x, f);
    warps.stage::<C>(x, f)
}

/// The chain for `active` stages, in order and padded with 0.
fn select(active: [u8; 3]) -> Option<ChainFn> {
    const F: u8 = FOLD;
    const C: u8 = CHEBY;
    const S: u8 = SEGMENT;
    Some(match active {
        [F, 0, 0] => chain::<F, 0, 0>,
        [C, 0, 0] => chain::<C, 0, 0>,
        [S, 0, 0] => chain::<S, 0, 0>,
        [F, C, 0] => chain::<F, C, 0>,
        [F, S, 0] => chain::<F, S, 0>,
        [C, F, 0] => chain::<C, F, 0>,
        [C, S, 0] => chain::<C, S, 0>,
        [S, F, 0] => chain::<S, F, 0>,
        [S, C, 0] => chain::<S, C, 0>,
        [F, C, S] => chain::<F, C, S>,
        [F, S, C] => chain::<F, S, C>,
        [C, F, S] => chain::<C, F, S>,
        [C, S, F] => chain::<C, S, F>,
        [S, F, C] => chain::<S, F, C>,
        [S, C, F] => chain::<S, C, F>,
        _ => return None,
    })
}

/// Samples per control block: pitch and crossfade update once per block.
pub(super) const CONTROL_BLOCK: usize = 8;

/// Settings shared by every voice of a lane for one control block.
pub(super) struct ControlContext {
    pub inv_sample_rate: f32,
    /// How much of each interval is a hard step rather than a crossfade, 0–0.5.
    pub crossfade_factor: f32,
    /// Per-block coefficient of the crossfade lowpass.
    pub crossfade_coef: f32,
}

/// Settings shared by every voice of a lane for one sample.
pub(super) struct VoiceContext<'a> {
    /// The phase warp with its stage values for this lane.
    pub phase: Option<(&'a PhaseWarp, ResolvedWarp)>,
    pub warps: WarpChain,
    /// Cross-FM depth, before per-oscillator antialiasing.
    pub cross_fm: f32,
    pub fade_inc: f32,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Oscillator {
    pub phase: f32,
    warp: PhaseWarpState,
    /// The V/Oct pitch `freq_norm` was computed from; `NaN` forces a
    /// recomputation.
    pitch: f32,
    freq_norm: f32,
    /// Antialiasing gains for `freq_norm`: the level fade towards Nyquist and
    /// the cross-FM depth rolloff.
    nyquist_fade: f32,
    fm_rolloff: f32,
    /// This sample's cross-FM rolloff, ramping linearly to `fm_rolloff` by
    /// `fm_rolloff_step` per sample over the block.
    fm_rolloff_now: f32,
    fm_rolloff_step: f32,
}

#[derive(Clone, Copy)]
pub(super) struct Voice {
    pub osc: [Oscillator; 2],
    /// Lowpassed crossfade at the end of the current block; `NaN` until the
    /// voice first sounds, so it starts on its target.
    crossfade: f32,
    /// This sample's crossfade, ramping linearly to `crossfade` by
    /// `crossfade_step` per sample over the block.
    crossfade_now: f32,
    crossfade_step: f32,
    pub fade: FadeIn,
    /// The scale lookup for `pair_root` in scale generation `pair_generation`;
    /// a `NaN` root forces a fresh lookup.
    pair: PitchPair,
    pair_root: f32,
    pair_generation: u64,
}

impl Default for Voice {
    fn default() -> Self {
        let osc = Oscillator {
            pitch: f32::NAN,
            ..Default::default()
        };
        Self {
            osc: [osc; 2],
            crossfade: f32::NAN,
            crossfade_now: 0.0,
            crossfade_step: 0.0,
            fade: FadeIn::default(),
            pair: PitchPair {
                p1: 0.0,
                p2: 0.0,
                crossfade: 0.0,
            },
            pair_root: f32::NAN,
            pair_generation: 0,
        }
    }
}

impl Voice {
    /// The scale notes around `root`, looked up only when it or the scale
    /// changed.
    #[inline]
    pub fn pitch_pair(&mut self, scale: &ScaleNotes, root: f32) -> PitchPair {
        if root != self.pair_root || scale.generation != self.pair_generation {
            self.pair = scale.pitch_pair(root);
            self.pair_root = root;
            self.pair_generation = scale.generation;
        }
        self.pair
    }

    /// Restart the voice silent, as when it joins the ensemble.
    pub fn restart(&mut self) {
        self.crossfade = f32::NAN;
        self.fade = FadeIn::default();
    }

    /// Start a control block. `pitches` are the pair's V/Oct pitches and
    /// `crossfade` the scale position between them.
    pub fn control(&mut self, pitches: [f32; 2], crossfade: f32, ctx: &ControlContext) {
        // Shape the crossfade so that notes are easier to find: the middle
        // `1 − 2·factor` of each interval fades, the rest holds a note.
        let width = 1.0 - 2.0 * ctx.crossfade_factor;
        let target = if width > 1e-6 {
            ((crossfade - ctx.crossfade_factor) / width).clamp(0.0, 1.0)
        } else if crossfade < 0.5 {
            0.0
        } else {
            1.0
        };
        let (from, to) = if self.crossfade.is_nan() {
            (target, target)
        } else {
            let next = self.crossfade + (target - self.crossfade) * ctx.crossfade_coef;
            let next = if (target - next).abs() < 1e-6 {
                target
            } else {
                next
            };
            (self.crossfade, next)
        };
        self.crossfade = to;
        self.crossfade_now = from;
        self.crossfade_step = (to - from) / CONTROL_BLOCK as f32;

        for (osc, pitch) in self.osc.iter_mut().zip(pitches) {
            let fresh = osc.pitch.is_nan();
            let from = osc.fm_rolloff;
            if pitch != osc.pitch {
                // An oscillator at or above Nyquist is silent, so the cap only
                // keeps an overflowing pitch from making the phase non-finite.
                let f = (voct_to_hz(pitch) * ctx.inv_sample_rate).min(0.5);
                let rolloff = (1.0 - 2.0 * f).max(0.0);
                osc.pitch = pitch;
                osc.freq_norm = f;
                osc.nyquist_fade = (1.0 - 4.0 * f * f).max(0.0);
                osc.fm_rolloff = rolloff * rolloff;
            }
            osc.fm_rolloff_now = if fresh { osc.fm_rolloff } else { from };
            osc.fm_rolloff_step = (osc.fm_rolloff - osc.fm_rolloff_now) / CONTROL_BLOCK as f32;
        }
    }

    /// Advance one sample. `mod_in` is the incoming cross-FM phase offset.
    /// Returns the voice's sample in `[-1, 1]` and its cross-FM output.
    #[inline]
    pub fn process(&mut self, mod_in: f32, ctx: &VoiceContext) -> (f32, f32) {
        self.crossfade_now += self.crossfade_step;
        let cf = self.crossfade_now;

        // The silent oscillator follows the sounding one, so it fades in
        // phase-aligned.
        if cf == 0.0 {
            self.osc[1].phase = self.osc[0].phase;
        } else if cf == 1.0 {
            self.osc[0].phase = self.osc[1].phase;
        }

        let fades = [1.0 - cf, cf];
        let mut sample = 0.0;
        let mut mod_out = 0.0;
        for (osc, fade) in self.osc.iter_mut().zip(fades) {
            let f = osc.freq_norm;
            osc.phase += f;
            osc.phase -= osc.phase.floor();

            let fade = fade * osc.nyquist_fade;
            osc.fm_rolloff_now += osc.fm_rolloff_step;
            let depth = ctx.cross_fm * osc.fm_rolloff_now;

            let mut s = 0.0;
            if fade > 0.0 {
                let mut p = osc.phase + mod_in;
                p -= p.floor();
                if let Some((warp, resolved)) = &ctx.phase {
                    p = warp.evaluate(p, resolved, f, &mut osc.warp);
                }
                s = sin_turns(p);
                if ctx.warps.active() {
                    s = ctx.warps.apply(s, f);
                }
                s *= fade;
            }
            mod_out += (s + 1.0) * depth;
            sample += s;
        }
        (sample * self.fade.advance(ctx.fade_inc), mod_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control_ctx() -> ControlContext {
        ControlContext {
            inv_sample_rate: 1.0 / 48_000.0,
            crossfade_factor: 0.0,
            crossfade_coef: 1.0,
        }
    }

    fn voice_ctx(cross_fm: f32) -> VoiceContext<'static> {
        VoiceContext {
            phase: None,
            warps: WarpChain::default(),
            cross_fm,
            fade_inc: 1.0,
        }
    }

    #[test]
    fn an_overflowing_pitch_keeps_the_phase_finite() {
        let mut voice = Voice::default();
        voice.control([440.0, f32::INFINITY], 0.5, &control_ctx());
        for _ in 0..CONTROL_BLOCK {
            voice.process(0.0, &voice_ctx(0.0));
        }
        assert!(voice.osc.iter().all(|osc| osc.phase.is_finite()));
    }

    #[test]
    fn cross_fm_depth_ramps_across_a_retune() {
        let ctx = voice_ctx(1.0);
        let mut voice = Voice::default();
        voice.control([0.0, 0.0], 0.0, &control_ctx());
        // With no crossfade to oscillator 1 it is silent, so its share of the
        // cross-FM output is its depth alone.
        let depth = |voice: &mut Voice| {
            let before = voice.osc[0];
            let (_, both) = voice.process(0.0, &ctx);
            let mut alone = Voice::default();
            alone.osc[0] = before;
            alone.osc[1].fm_rolloff_now = 0.0;
            alone.crossfade_now = 0.0;
            both - alone.process(0.0, &ctx).1
        };
        let settled = depth(&mut voice);

        voice.control([0.0, 5.0], 0.0, &control_ctx());
        let mut previous = settled;
        let mut largest_step = 0.0f32;
        for _ in 0..CONTROL_BLOCK {
            let now = depth(&mut voice);
            largest_step = largest_step.max((now - previous).abs());
            previous = now;
        }
        let total = (previous - settled).abs();
        assert!(total > 0.1, "retune moved the depth by {total}");
        assert!(largest_step < total / CONTROL_BLOCK as f32 * 1.01);
    }
}
