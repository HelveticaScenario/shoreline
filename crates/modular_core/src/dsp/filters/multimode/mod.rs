//! `$multimode` — one channel of Mutable Instruments Blades, modelled from its
//! published schematic (blades_v40).
//!
//! Signal path: drive VCA → zener-clamped transimpedance amp → clean/wavefolded
//! crossfade → 2164-based state-variable filter → LP/BP/HP morphing VCAs.
//! Every V2164 control port here is driven through a linearising servo or a
//! precision rectifier whose diode keeps the control voltage ≥ 0 V, so each
//! VCA's gain is clamped to at most unity except the drive VCA, which has no
//! such diode.

mod components;
mod controls;
mod curve;
mod drive;
mod group;
mod lanes;
mod output;
mod svf;
mod vca;

use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::utils::rng::seed_base,
    poly::{PolyOutput, PolySignal},
};

use controls::{ControlInputs, update_controls};
use group::{GroupOutputs, GroupState, Tables, process_block_group, process_group, step};
use lanes::{HalfbandDown, LANES, f32x4};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MultimodeParams {
    #[input("the sound to filter; DC-coupled, so CV passes too", type = audio)]
    input: PolySignal,
    #[input(
        "cutoff frequency; the top octaves track flat, so the top of the range sounds near 21 kHz",
        type = frequency,
        default = 0.0,
        range = (-10.08, 6.53),
    )]
    cutoff: PolySignal,
    #[input(
        "filter resonance; self-oscillates from about 4.3",
        type = control,
        default = 0.0,
        range = (0.0, ..),
        control_range = (0.0, 5.0),
    )]
    #[deserr(default)]
    resonance: Option<PolySignal>,
    #[input(
        "response: 0 lowpass, 2.5 bandpass, 5 highpass, crossfading between",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    mode: Option<PolySignal>,
    #[input(
        "input level into the clipper: 0 unity, 5 +18 dB, -5 silent; maxes out at +20 dB",
        type = control,
        default = 0.0,
        range = (-5.0, 5.7),
        control_range = (0.0, 5.0),
    )]
    #[deserr(default)]
    drive: Option<PolySignal>,
    #[input(
        "drive character: 0 soft clipping, 5 two-stage wavefolder",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    fold: Option<PolySignal>,
    #[input(
        "run at twice the engine rate: less aliasing from clipping, folding and self-oscillation, and a truer highpass at high cutoffs; false costs less CPU"
    )]
    #[serde(default = "default_true")]
    #[deserr(default = default_true())]
    high_res: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MultimodeOutputs {
    #[output("output", "response selected by mode", default, range = (-10.0, 10.0))]
    sample: PolyOutput,
    #[output("lp", "lowpass, the output at mode 0", range = (-10.0, 10.0))]
    lp: PolyOutput,
    #[output("bp", "bandpass, the output near mode 2.5", range = (-10.0, 10.0))]
    bp: PolyOutput,
    #[output("hp", "highpass, the output at mode 5", range = (-10.0, 10.0))]
    hp: PolyOutput,
}

const MULTIMODE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Blades-style multimode filter",
    description: "A circuit model of one channel of Mutable Instruments Blades: a filter that \
                  morphs from lowpass through bandpass to highpass, after a drive stage that \
                  soft-clips or wavefolds. It self-oscillates into a sine, takes audio-rate \
                  cutoff modulation for FM, and is DC-coupled, so it can shape CV too.",
    categories: &[ModuleCategory::Filter, ModuleCategory::Distortion],
    sections: &[],
    examples: &[
        DocExample {
            title: "Resonant sweep morphing from lowpass to highpass",
            code: "$saw('c2').$.multimode($sine('0.1hz').range(-1, 2), 3.8, $sine('0.07hz').range(0, 5)).out()",
        },
        DocExample {
            title: "Wavefolded sine",
            code: "$sine('c3').$.multimode('c6', 1, 0, { drive: 3, fold: 3 }).out()",
        },
        DocExample {
            title: "Self-oscillating sine at A3",
            code: "$multimode(0, 'a3', 5).out()",
        },
    ],
};

#[module(
    name = "$multimode",
    docs = MULTIMODE_DOCS,
    args(input, cutoff, resonance, mode),
    has_init,
    patch_update,
    block
)]
pub struct Multimode {
    outputs: MultimodeOutputs,
    state: MultimodeState,
    channel_state: Box<[GroupState]>,
    params: MultimodeParams,
}

#[derive(Default)]
struct MultimodeState {
    sample_rate: f32,
    /// Channel count the groups' lanes were last set up for.
    channels: usize,
}

/// `read(channel)` for the first `active` channels from `base`, 0 for dead
/// lanes.
#[inline]
fn lanes(base: usize, active: usize, read: impl Fn(usize) -> f32) -> [f32; LANES] {
    std::array::from_fn(|lane| {
        if lane < active {
            read(base + lane)
        } else {
            0.0
        }
    })
}

impl Multimode {
    fn init(&mut self, sample_rate: f32) {
        components::prime();
        drive::prime();
        output::prime();
        self.state.sample_rate = sample_rate;
        let base = seed_base(self);
        for (group, state) in self.channel_state.iter_mut().enumerate() {
            for (lane, noise) in state.noise.iter_mut().enumerate() {
                noise.seed(base, group * LANES + lane);
            }
        }
    }

    fn core_rate(&self) -> f32 {
        if self.params.high_res {
            2.0 * self.state.sample_rate
        } else {
            self.state.sample_rate
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let (rate, high_res) = (self.core_rate(), self.params.high_res);
        let tables = Tables::get();
        let (p, outputs) = (&self.params, &mut self.outputs);
        let write =
            |outputs: &mut MultimodeOutputs, base: usize, active: usize, out: GroupOutputs| {
                let [sample, lp, bp, hp] = out;
                for lane in 0..active {
                    let ch = base + lane;
                    outputs.sample.set(ch, sample[lane]);
                    outputs.lp.set(ch, lp[lane]);
                    outputs.bp.set(ch, bp[lane]);
                    outputs.hp.set(ch, hp[lane]);
                }
            };
        // A single channel runs on lane 0 alone, without three dead lanes.
        if channels == 1 {
            let state = &mut self.channel_state[0];
            update_controls::<f32>(&mut state.controls, &ControlInputs::read(p, 0, 1), 1, rate);
            let input = lanes(0, 1, |ch| p.input.get_value(ch));
            let out = step::<f32>(state, input, 1, high_res, tables);
            write(outputs, 0, 1, out);
            return;
        }
        for (group, state) in self
            .channel_state
            .iter_mut()
            .take(channels.div_ceil(LANES))
            .enumerate()
        {
            let base = group * LANES;
            let active = (channels - base).min(LANES);
            let controls = ControlInputs::read(p, base, active);
            let input = lanes(base, active, |ch| p.input.get_value(ch));
            let group = (active, high_res, rate, tables);
            // A group's one live channel runs on lane 0 as well.
            let out = if active == 1 {
                process_group::<f32>(state, &controls, input, group)
            } else {
                process_group::<f32x4>(state, &controls, input, group)
            };
            write(outputs, base, active, out);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut MultimodeBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let (rate, high_res) = (self.core_rate(), self.params.high_res);
        let tables = Tables::get();
        let mut scratch = Default::default();
        let inputs = self
            .params
            .block_inputs(range.clone(), channels, &mut scratch)?;
        for (group, state) in self
            .channel_state
            .iter_mut()
            .take(channels.div_ceil(LANES))
            .enumerate()
        {
            let base = group * LANES;
            let active = (channels - base).min(LANES);
            let group = (base, active, high_res, rate, tables);
            // A group's one live channel runs on lane 0, as in `update`.
            if active == 1 {
                process_block_group::<f32>(state, &inputs, outputs, range.clone(), group);
            } else {
                process_block_group::<f32x4>(state, &inputs, outputs, range.clone(), group);
            }
        }
        Some(())
    }
}

impl crate::types::PatchUpdateHandler for Multimode {
    fn on_patch_update(&mut self) {
        let rate = self.core_rate();
        let (old, channels) = (self.state.channels, self.channel_count());
        let groups = channels.div_ceil(LANES);
        for group in self.channel_state.iter_mut().take(groups) {
            group.configure(rate);
        }
        // A group with one live channel keeps that channel's four output
        // downsamplers in one downsampler's lanes; a group moving between one
        // live channel and several transposes them. A group that comes back
        // with one live channel starts all four from rest, since clearing its
        // lane alone would leave the other three outputs' history.
        let live = |count: usize, group: usize| count.saturating_sub(group * LANES).min(LANES);
        for (group, state) in self.channel_state.iter_mut().enumerate() {
            let (before, after) = (live(old, group), live(channels, group));
            if before != 0 && after != 0 && (before == 1) != (after == 1) {
                HalfbandDown::transpose(&mut state.down);
            } else if before == 0 && after == 1 {
                state.down = Default::default();
            }
        }
        // Channels new to this patch start from rest, as they would in a fresh
        // module, rather than from a lane that ran dead before.
        for ch in old.min(channels)..channels {
            self.channel_state[ch / LANES].clear_lane(ch % LANES);
        }
        // A new channel count changes which lanes are dead; recomputing every
        // law sets them to follow lane 0 again.
        if old != channels {
            for group in self.channel_state.iter_mut().take(groups) {
                group.controls.clear();
            }
        }
        self.state.channels = channels;
    }
}

message_handlers!(impl Multimode {});

#[cfg(test)]
mod tests;
