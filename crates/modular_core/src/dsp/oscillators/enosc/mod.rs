//! `$enosc` — the core of the 4ms Ensemble Oscillator: per lane, a bank of
//! sine voices spread across a scale, with cross FM, a phase warp, and the
//! fold/cheby/segment warps.
//!
//! Adapted from the 4ms Ensemble Oscillator. Copyright 4ms Company. Used under
//! GPL v3.

mod scale;
mod voice;

use deserr::Deserr;
use schemars::JsonSchema;
use serde::Serialize;

use crate::dsp::fx::enosc_tables;
use crate::dsp::phase_warp::shapes::knob;
use crate::dsp::phase_warp::{PhaseWarp, PhaseWarpGlides};
use crate::dsp::utils::dc_blocker::DcBlocker;
use crate::dsp::utils::fade::{DEFAULT_FADE_IN_SECS, FadeIn};
use crate::dsp::utils::rng::{LcgRng, seed_base};
use crate::dsp::utils::sanitize;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PORT_MAX_LANES, PolyOutput, PolySignal, PolySignalExt};
use crate::types::SwapGlide;

use scale::{EnoscScale, ScaleNotes};
use voice::{CONTROL_BLOCK, ControlContext, EffectOrder, Voice, VoiceContext, WarpChain};

/// Most voices per lane.
const MAX_VOICES: usize = 16;

/// Gain correction for a sum of `n` uncorrelated voices, indexed by `n`: how far
/// the sum can be pushed before it is likely to exceed full scale. From the
/// reference's `normalization_factors` table.
const NORMALIZATION: [f32; MAX_VOICES + 1] = [
    0.0,
    -0.4,
    0.001_303_781,
    0.000_977_517_1,
    0.003_134_796_2,
    0.007_874_016,
    0.018_181_818,
    0.031_218_53,
    0.047_748_977,
    0.066_666_67,
    0.087_678_64,
    0.109_425_79,
    0.132_743_36,
    0.156_502_1,
    0.180_811_81,
    0.205_414_95,
    0.230_073_5,
];

/// How each voice's cross-FM output reaches the others.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, Serialize, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum CrossFmMode {
    /// The lowest voice modulates every other voice.
    #[default]
    Up,
    /// Each voice modulates the next one up.
    Chain,
    /// The highest voice modulates every other voice, one sample late.
    Down,
}

impl CrossFmMode {
    fn depth_scale(self) -> f32 {
        match self {
            CrossFmMode::Up => 6.0,
            CrossFmMode::Chain => 0.9,
            CrossFmMode::Down => 4.0,
        }
    }
}

fn default_count() -> usize {
    8
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct EnoscParams {
    #[input("pitch of the scale's first note; every voice moves with it", type = pitch)]
    pitch: PolySignal,
    #[input("the scale the voices sit on, built with `$enosc.scale.*`; every semitone if omitted")]
    #[deserr(default)]
    scale: Option<EnoscScale>,
    #[input(
        "lowest voice's offset from `pitch`, one octave per volt, quantized to `scale`; each voice above adds `spread`",
        type = control,
        default = 0.0,
        control_range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    root: Option<PolySignal>,
    #[input(
        "distance between neighbouring voices, up to 7.5 semitones",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    spread: Option<PolySignal>,
    #[input(
        "unquantized detuning that grows and alternates in sign up the voices",
        type = control,
        default = 1.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    detune: Option<PolySignal>,
    #[input(
        "how much of each step between scale notes crossfades rather than jumps: 0 hard steps, 5 always crossfading",
        type = control,
        default = 4.33,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    crossfade: Option<PolySignal>,
    #[input("cross-FM depth", type = control, default = 0.0, range = (0.0, 5.0))]
    #[deserr(default)]
    cross_fm: Option<PolySignal>,
    #[input("how each voice's cross FM reaches the others")]
    #[serde(default)]
    #[deserr(default)]
    cross_fm_mode: CrossFmMode,
    #[input(
        "level tilt across the voices: negative favours the lowest, positive the highest",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    balance: Option<PolySignal>,
    #[input("phase warp applied to every oscillator before its sine, built with `$phase.*`")]
    #[deserr(default)]
    phase: Option<PhaseWarp>,
    #[input("wavefold amount", type = control, default = 0.0, range = (0.0, 5.0))]
    #[deserr(default)]
    fold: Option<PolySignal>,
    #[input("Chebyshev waveshaping amount", type = control, default = 0.0, range = (0.0, 5.0))]
    #[deserr(default)]
    cheby: Option<PolySignal>,
    #[input("triangle-segment waveshaping amount", type = control, default = 0.0, range = (0.0, 5.0))]
    #[deserr(default)]
    segment: Option<PolySignal>,
    #[input("order of the fold (f), cheby (c) and segment (s) warps, e.g. \"fcs\" or \"scf\"")]
    #[serde(default)]
    #[deserr(default)]
    effect_order: EffectOrder,
    #[input("voices per lane", range = (1, 16))]
    #[serde(default = "default_count")]
    #[deserr(default = default_count())]
    count: usize,
}

/// As wide as the widest input. The scale is one shared set of notes, so it
/// does not widen the module.
#[allow(private_interfaces)]
pub fn enosc_derive_lane_count(params: &EnoscParams) -> usize {
    params
        .pitch
        .lanes()
        .max(params.root.lane_count())
        .max(params.spread.lane_count())
        .max(params.detune.lane_count())
        .max(params.crossfade.lane_count())
        .max(params.cross_fm.lane_count())
        .max(params.balance.lane_count())
        .max(params.phase.as_ref().map_or(0, PhaseWarp::lanes))
        .max(params.fold.lane_count())
        .max(params.cheby.lane_count())
        .max(params.segment.lane_count())
        .clamp(1, PORT_MAX_LANES)
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct EnoscOutputs {
    #[output(
        "output",
        "the voices in stereo: 1, 3, 5, … on the left and 2, 4, 6, … on the right",
        type = cv,
        default,
        range = (-5.0, 5.0),
    )]
    output: PolyOutput,
    #[output("all", "mix of every voice", type = cv, range = (-5.0, 5.0), mono)]
    all: PolyOutput,
    #[output("odd", "mix of voices 1, 3, 5, …", type = cv, range = (-5.0, 5.0), mono)]
    odd: PolyOutput,
    #[output("even", "mix of voices 2, 4, 6, …", type = cv, range = (-5.0, 5.0), mono)]
    even: PolyOutput,
    #[output("low", "mix of the lower half of the voices", type = cv, range = (-5.0, 5.0), mono)]
    low: PolyOutput,
    #[output("high", "mix of the upper half of the voices", type = cv, range = (-5.0, 5.0), mono)]
    high: PolyOutput,
    #[output("root", "the lowest voice alone", type = cv, range = (-5.0, 5.0), mono)]
    root: PolyOutput,
    #[output("other", "mix of every voice but the lowest", type = cv, range = (-5.0, 5.0), mono)]
    other: PolyOutput,
}

/// Number of mono mixes, each with its own DC blocker.
const OUTPUTS: usize = 7;

#[derive(Clone, Copy, Default)]
struct LaneState {
    voices: [Voice; MAX_VOICES],
    /// Voices that sounded on the previous sample; the rest restart silent.
    active_voices: usize,
    /// Samples left in the current control block; 0 starts a new one.
    control_left: usize,
    /// The highest voice's cross-FM output from the previous sample, for
    /// [`CrossFmMode::Down`].
    down_bus: f32,
    /// One per mono mix, then the stereo output's left and right.
    dc: [DcBlocker; OUTPUTS + 2],
    cross_fm: SwapGlide,
    balance: SwapGlide,
    balance_ratio: BalanceRatio,
    warps: WarpChain,
    fold: SwapGlide,
    cheby: SwapGlide,
    segment: SwapGlide,
    phase: PhaseWarpGlides,
}

/// The level ratio between neighbouring voices for a `balance` setting,
/// recomputed only when the setting changes.
#[derive(Clone, Copy)]
struct BalanceRatio {
    volts: f32,
    ratio: f32,
}

impl Default for BalanceRatio {
    fn default() -> Self {
        Self {
            volts: 0.0,
            ratio: 1.0,
        }
    }
}

impl BalanceRatio {
    #[inline]
    fn get(&mut self, volts: f32) -> f32 {
        if volts != self.volts {
            let tilt = (volts / 5.0).clamp(-1.0, 1.0);
            self.volts = volts;
            self.ratio = f32::exp2(4.0 * tilt * tilt * tilt);
        }
        self.ratio
    }
}

#[derive(Default)]
struct EnoscState {
    /// Voices per lane, the cross-FM depth scale for that many, and the mix
    /// normalization for that many; set from the params in `on_patch_update`.
    count: usize,
    depth_scale: f32,
    normalization: f32,
    inv_sample_rate: f32,
    dc_coeff: f32,
    crossfade_coef: f32,
    fade_inc: f32,
}

const ENOSC_DOCS: ModuleDocs = ModuleDocs {
    summary: "Ensemble of sines on a scale",
    description: "A bank of sine voices stacked up a scale, after the 4ms Ensemble \
                  Oscillator. Sweeping `root` or `spread` morphs smoothly from chord \
                  to chord, and cross FM, phase warps and the fold, cheby and segment \
                  shapers turn the sines into anything from glassy pads to snarling \
                  drones. The output is stereo, odd voices left and even voices \
                  right, and the other outputs split the voices the way the \
                  hardware's stereo modes do. Inputs are read as mono.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Minor chord",
            code: "$enosc('c3', $enosc.scale.minor).out()",
        },
        DocExample {
            title: "Harmonic stack",
            code: r#"
                $enosc('c2', $enosc.scale.harm4to7, {
                  spread: 1,
                  count: 6,
                }).out()
            "#,
        },
        DocExample {
            title: "Custom scale",
            code: "$enosc('c2', $enosc.scale.custom('0 2 3 7 9', true)).out()",
        },
        DocExample {
            title: "Stereo pulsar drone",
            code: r#"
                $enosc('a2', $enosc.scale.just6, {
                  spread: $sine('0.05hz').range(1, 3),
                  crossFm: 1.5,
                  phase: $phase.pulsar(2),
                  fold: 1,
                }).out()
            "#,
        },
    ],
};

#[module(
    name = "$enosc",
    docs = ENOSC_DOCS,
    lanes_derive = enosc_derive_lane_count,
    stereo = stereo,
    has_init,
    patch_update,
    args(pitch, scale)
)]
pub struct Enosc {
    outputs: EnoscOutputs,
    state: EnoscState,
    lane_state: Box<[LaneState]>,
    params: EnoscParams,
}

impl Enosc {
    /// Sample-rate-only constants and the random starting phases. Phases live in
    /// `lane_state`, which a patch update carries over lane by lane, so
    /// only fresh lanes keep these.
    fn init(&mut self, sample_rate: f32) {
        enosc_tables::prime();
        self.state.inv_sample_rate = 1.0 / sample_rate;
        // The reference's DC blocker pole is 0.995 at 48 kHz.
        self.state.dc_coeff = 0.995f32.powf(48_000.0 / sample_rate);
        // Matches the reference's crossfade glide: coefficient 0.1 applied 6,000
        // times a second (once per 8 samples at 48 kHz), scaled to this
        // module's control rate.
        let control_rate = sample_rate / CONTROL_BLOCK as f32;
        self.state.crossfade_coef = 1.0 - 0.9f32.powf(6_000.0 / control_rate);
        self.state.fade_inc = FadeIn::increment(DEFAULT_FADE_IN_SECS, sample_rate);

        let mut rng = LcgRng::default();
        rng.seed(seed_base(self), 0);
        for lane in self.lane_state.iter_mut() {
            for voice in lane.voices.iter_mut() {
                for osc in voice.osc.iter_mut() {
                    osc.phase = rng.next_unit();
                }
            }
        }
    }

    fn update(&mut self, sample_rate: f32) {
        // Lane `l` side `c` is lane `l * 2 + c`. The voices run once per lane
        // and read each input folded; their state lives on the left side.
        let lanes = self.lane_count() / 2;
        let params = &self.params;
        let state = &self.state;
        let scale = params
            .scale
            .as_ref()
            .map_or(&ScaleNotes::CHROMATIC, |s| &s.0);

        let count = state.count;

        for lane in 0..lanes {
            let at = lane * 2;
            let folded = |signal: &Option<PolySignal>, default: f32| {
                0.5 * (signal.value_or(at, default) + signal.value_or(at + 1, default))
            };
            let cs = &mut self.lane_state[at];
            if count > cs.active_voices {
                for voice in &mut cs.voices[cs.active_voices..count] {
                    voice.restart();
                }
                cs.control_left = 0;
            }
            cs.active_voices = count;

            if cs.control_left == 0 {
                cs.control_left = CONTROL_BLOCK;
                let pitch =
                    sanitize(0.5 * (params.pitch.get_value(at) + params.pitch.get_value(at + 1)));
                let root = sanitize(folded(&params.root, 0.0));
                let spread = knob(folded(&params.spread, EnoscParams::SPREAD_DEFAULT)) * 7.5 / 12.0;
                let detune = knob(folded(&params.detune, EnoscParams::DETUNE_DEFAULT)).powi(4)
                    * (10.0 / 16.0)
                    / 12.0;
                let crossfade = knob(folded(&params.crossfade, EnoscParams::CROSSFADE_DEFAULT));
                let ctx = ControlContext {
                    inv_sample_rate: state.inv_sample_rate,
                    crossfade_factor: 0.5 * (1.0 - crossfade * crossfade),
                    crossfade_coef: state.crossfade_coef,
                };
                let mut voice_root = root;
                let mut detune_step = detune;
                let mut detune_offset = 0.0f32;
                for voice in &mut cs.voices[..count] {
                    let pair = voice.pitch_pair(scale, voice_root);
                    let offset = pitch + detune_offset;
                    voice_root += spread;
                    detune_step *= -1.2;
                    detune_offset += detune_step;
                    voice.control([pair.p1 + offset, pair.p2 + offset], pair.crossfade, &ctx);
                }
            }
            cs.control_left -= 1;

            let cross_fm = cs.cross_fm.follow(
                folded(&params.cross_fm, 0.0),
                params.cross_fm.glide_mark(at),
                sample_rate,
            );
            // Ignore the bottom 1% so CV noise cannot add harmonics.
            let cross_fm = ((knob(cross_fm) - 0.01) / 0.99).max(0.0) * state.depth_scale;

            let balance = cs.balance.follow(
                folded(&params.balance, 0.0),
                params.balance.glide_mark(at),
                sample_rate,
            );
            let balance = cs.balance_ratio.get(balance);

            let glided = |glide: &mut SwapGlide, signal: &Option<PolySignal>| {
                knob(glide.follow(folded(signal, 0.0), signal.glide_mark(at), sample_rate))
            };
            cs.warps.set(
                params.effect_order,
                glided(&mut cs.fold, &params.fold),
                glided(&mut cs.cheby, &params.cheby),
                glided(&mut cs.segment, &params.segment),
            );

            let phase = params
                .phase
                .as_ref()
                .map(|warp| (warp, warp.resolve(at, sample_rate, &mut cs.phase)));

            let ctx = VoiceContext {
                phase,
                warps: cs.warps,
                cross_fm,
                fade_inc: state.fade_inc,
            };

            let mut sums = [0.0f32; OUTPUTS];
            let [all, odd, even, low, high, lowest, other] = &mut sums;
            let mut amplitude = 1.0f32;
            let mut amplitude_sum = 0.0f32;
            let mut bus = 0.0f32;
            let mut next_down_bus = 0.0f32;

            for (i, voice) in cs.voices[..count].iter_mut().enumerate() {
                let top = i == count - 1;
                let mod_in = match params.cross_fm_mode {
                    CrossFmMode::Up | CrossFmMode::Chain if i == 0 => 0.0,
                    CrossFmMode::Up | CrossFmMode::Chain => bus,
                    CrossFmMode::Down if top => 0.0,
                    CrossFmMode::Down => cs.down_bus,
                };
                let (sample, mod_out) = voice.process(mod_in, &ctx);
                match params.cross_fm_mode {
                    CrossFmMode::Up if i == 0 => bus = mod_out,
                    CrossFmMode::Chain => bus = mod_out,
                    CrossFmMode::Down if top => next_down_bus = mod_out,
                    _ => {}
                }

                let v = sample * amplitude;
                amplitude_sum += amplitude;
                amplitude *= balance;
                *all += v;
                if i % 2 == 0 {
                    *odd += v;
                } else {
                    *even += v;
                }
                if i < count / 2 {
                    *low += v;
                } else {
                    *high += v;
                }
                if i == 0 {
                    *lowest += v;
                } else {
                    *other += v;
                }
            }
            cs.down_bus = next_down_bus;
            let (odd_sum, even_sum) = (*odd, *even);

            // Normalize the mix, giving back level as the balance evens out
            // (more voices contribute and their sum is less likely to peak).
            let evenness = if balance <= 1.0 {
                balance
            } else {
                1.0 / balance
            };
            let atten = (0.5 + (0.5 + state.normalization) * evenness) / amplitude_sum;
            let gains = [atten, atten, atten, atten, atten, 0.7, 0.6 * atten];
            let [.., dc_left, dc_right] = &mut cs.dc;
            self.outputs
                .output
                .set(at, dc_left.process(odd_sum * atten * 5.0, state.dc_coeff));
            self.outputs.output.set(
                at + 1,
                dc_right.process(even_sum * atten * 5.0, state.dc_coeff),
            );
            let outputs = [
                &mut self.outputs.all,
                &mut self.outputs.odd,
                &mut self.outputs.even,
                &mut self.outputs.low,
                &mut self.outputs.high,
                &mut self.outputs.root,
                &mut self.outputs.other,
            ];
            for (((out, sum), gain), dc) in outputs.into_iter().zip(sums).zip(gains).zip(&mut cs.dc)
            {
                out.set(lane, dc.process(sum * gain * 5.0, state.dc_coeff));
            }
        }
    }
}

impl crate::types::PatchUpdateHandler for Enosc {
    fn on_patch_update(&mut self) {
        let count = self.params.count.clamp(1, MAX_VOICES);
        self.state.count = count;
        self.state.depth_scale = 6.0 / count as f32 * self.params.cross_fm_mode.depth_scale();
        self.state.normalization = NORMALIZATION[count];
    }
}

message_handlers!(impl Enosc {});
