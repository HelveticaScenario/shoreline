use std::ops::Range;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    block_input::{InputScratch, OptionBlockInput},
    dsp::utils::sanitize,
    poly::{LaneSum, LaneSumExt, PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct StereoMixerParams {
    #[input("the signal to place in the stereo field", type = audio)]
    input: PolySignal,
    #[input(
        "pan position of each lane: -5 left, 0 centre, +5 right",
        type = control,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    pan: Option<PolySignal>,
    #[input(
        "how far the lanes fan out around their pan positions: 0 none, 5 widest",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    width: Option<LaneSum>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct StereoMixerOutputs {
    #[output("output", "stereo mix: left on lane 0, right on lane 1", type = cv, default)]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct LaneState {
    pan: SwapGlide,
    /// The final pan the cached gains were computed for, and the
    /// `(left, right)` gains.
    gain_cache: Option<(f32, f32, f32)>,
}

const STEREO_MIX_DOCS: ModuleDocs = ModuleDocs {
    summary: "Stereo panner and spreader",
    description: "Places a signal in the stereo field and puts out left and right. Give each \
                  lane its own `pan`, or raise `width` to fan the voices of a chord out \
                  from the centre.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "Chord fanned across the stereo field",
        code: r#"
            $stereoMix($saw($chord('C3[m9]')).lpf('c5', 1), {
              width: 4,
            }).out()
        "#,
    }],
};

#[module(
    name = "$stereoMix",
    docs = STEREO_MIX_DOCS,
    lanes = 2,
    args(input),
    has_init,
    block
)]
pub struct StereoMixer {
    outputs: StereoMixerOutputs,
    params: StereoMixerParams,
    state: StereoMixerState,
    /// Per-input-lane pan state. Sized to the *input* signal's lane
    /// count (not the module's fixed 2 output lanes) in `init`, so its
    /// length is decoupled from the derived lane count. State transfer
    /// carries over the overlapping lanes; a width change keeps the surplus
    /// freshly initialised.
    lane_state: Box<[LaneState]>,
}

/// Module-level state for the StereoMixer.
#[derive(Default)]
struct StereoMixerState {
    /// Width buffer for stereo spread
    width_buffer: SwapGlide,
}

impl StereoMixer {
    fn init(&mut self, _sample_rate: f32) {
        // One pan glide per input lane.
        let input_lanes = self.params.input.lanes().max(1);
        self.lane_state = vec![LaneState::default(); input_lanes].into_boxed_slice();
    }

    /// Outside a glide, reads width and pan raw, exactly as `process_block`
    /// does.
    pub fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        let width = self.state.width_buffer.follow(
            p.width.value_or(0.0).clamp(0.0, 5.0),
            p.width.glide_mark(),
            sample_rate,
        );
        let (left, right) = mix_sample(
            &mut self.lane_state[..p.input.lanes()],
            width,
            |lane| p.input.get_value(lane),
            |lane, glide| {
                glide.follow(
                    p.pan.value_or_zero(lane).clamp(-5.0, 5.0),
                    p.pan.glide_mark(lane),
                    sample_rate,
                )
            },
        );
        self.outputs.sample.set(0, left);
        self.outputs.sample.set(1, right);
    }

    fn process_block(
        &mut self,
        outputs: &mut StereoMixerBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let p = &self.params;
        let input_lanes = p.input.lanes();
        let width_glide = &mut self.state.width_buffer;
        let lane_state = &mut self.lane_state[..input_lanes];
        let gliding = width_glide.will_glide(p.width.glide_mark())
            || lane_state
                .iter()
                .enumerate()
                .any(|(lane, state)| state.pan.will_glide(p.pan.glide_mark(lane)));
        if gliding {
            return None;
        }
        let mut scratch: [InputScratch; 3] = std::array::from_fn(|_| InputScratch::new());
        let [s0, s1, s2] = &mut scratch;
        let input = p.input.block_input(range.clone(), input_lanes, s0)?;
        let pan = p.pan.block_input_or(range.clone(), input_lanes, 0.0, s1)?;
        let width = match &p.width {
            Some(width) => {
                let width = width.poly();
                Some(width.block_input(range.clone(), width.lanes(), s2)?)
            }
            None => None,
        };
        let width_at = |i: usize| {
            width
                .map_or(0.0, |width| width.row(i).iter().sum::<f32>())
                .clamp(0.0, 5.0)
        };
        for i in range.clone() {
            let (input, pan) = (input.row(i), pan.row(i));
            let (left, right) = mix_sample(
                lane_state,
                width_at(i),
                |lane| input[lane],
                |lane, _| pan[lane].clamp(-5.0, 5.0),
            );
            let out = outputs.sample.slot_mut(i);
            out[0] = sanitize(left);
            out[1] = sanitize(right);
        }
        let last = range.end - 1;
        width_glide.settle(width_at(last), p.width.glide_mark());
        for (lane, (state, &pan)) in lane_state.iter_mut().zip(pan.row(last)).enumerate() {
            state
                .pan
                .settle(pan.clamp(-5.0, 5.0), p.pan.glide_mark(lane));
        }
        Some(())
    }
}

/// One sample of the stereo mix: each lane `input(lane)` placed at the base
/// pan `pan(lane, glide)` returns, spread by `width`, summed into `(left, right)`.
#[inline(always)]
fn mix_sample(
    lane_state: &mut [LaneState],
    width: f32,
    input: impl Fn(usize) -> f32,
    mut pan: impl FnMut(usize, &mut SwapGlide) -> f32,
) -> (f32, f32) {
    let input_lanes = lane_state.len();
    let mut left_sum = 0.0f32;
    let mut right_sum = 0.0f32;

    for (lane, state) in lane_state.iter_mut().enumerate() {
        let input = input(lane);
        let base_pan = pan(lane, &mut state.pan);

        // Voices spread from -width (voice 0) to +width (last voice) around
        // their base pan.
        let spread_offset = if input_lanes > 1 {
            let voice_pos = lane as f32 / (input_lanes - 1) as f32;
            (voice_pos - 0.5) * 2.0 * width
        } else {
            0.0
        };

        let (left_gain, right_gain) = state.gains((base_pan + spread_offset).clamp(-5.0, 5.0));
        left_sum += input * left_gain;
        right_sum += input * right_gain;
    }
    (left_sum, right_sum)
}

impl LaneState {
    /// The equal-power `(left, right)` gains for `pan`, cached while it holds.
    #[inline(always)]
    fn gains(&mut self, pan: f32) -> (f32, f32) {
        match self.gain_cache {
            Some((cached, left, right)) if cached == pan => (left, right),
            _ => {
                // -5..+5 to 0..1: 0 full left, 1 full right.
                let pan_norm = (pan + 5.0) / 10.0;
                let gains = ((1.0 - pan_norm).sqrt(), pan_norm.sqrt());
                self.gain_cache = Some((pan, gains.0, gains.1));
                gains
            }
        }
    }
}

message_handlers!(impl StereoMixer {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct as _, Signal};

    /// Build a StereoMixer the way the module macro would: fixed 2-lane
    /// stereo output, per-input-lane pan state sized by `init`. A fresh
    /// `SwapGlide` starts on its input, so a single call shows the full
    /// pan/width.
    fn make_stereo(input: PolySignal) -> StereoMixer {
        let mut outputs = StereoMixerOutputs::default();
        outputs.set_all_lanes(2);
        let mut m = StereoMixer {
            outputs,
            params: StereoMixerParams {
                input,
                pan: None,
                width: None,
            },
            state: StereoMixerState::default(),
            lane_state: Box::default(),
            _lane_count: 2,
            _block_index: Default::default(),
        };
        m.init(48000.0);
        m
    }

    fn poly(n: usize) -> PolySignal {
        PolySignal::poly(&(0..n).map(|_| Signal::Volts(0.0)).collect::<Vec<_>>())
    }

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "expected {b}, got {a}");
    }

    #[test]
    fn pan_state_sized_to_input_lanes() {
        assert_eq!(make_stereo(poly(5)).lane_state.len(), 5);
        assert_eq!(make_stereo(poly(1)).lane_state.len(), 1);
    }

    #[test]
    fn wide_input_does_not_panic_and_outputs_stereo() {
        let mut m = make_stereo(poly(40));
        m.update(48000.0);
        assert_eq!(m.outputs.sample.lanes(), 2);
    }

    #[test]
    fn test_default_width_is_no_spread() {
        // With no width signal connected, width defaults to 0 (no spread):
        // every voice stays centered, so a 2-voice input sums equally to both
        // lanes at the equal-power center gain sqrt(0.5). (A width of 5
        // would instead hard-pan voice 0 left and voice 1 right.)
        let mut m = make_stereo(PolySignal::poly(&[Signal::Volts(1.0), Signal::Volts(0.0)]));
        m.update(48000.0);
        let center = 0.5f32.sqrt();
        approx(m.outputs.sample.get(0), center); // left  = voice 0 centered
        approx(m.outputs.sample.get(1), center); // right = voice 0 centered
    }
}
