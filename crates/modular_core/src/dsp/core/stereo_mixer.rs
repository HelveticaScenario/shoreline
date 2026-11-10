use std::ops::Range;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    block_input::{InputScratch, OptionBlockInput},
    dsp::utils::sanitize,
    poly::{MonoSignal, MonoSignalExt, PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct StereoMixerParams {
    #[input("the signal to place in the stereo field", type = audio)]
    input: PolySignal,
    #[input(
        "pan position of each channel: -5 left, 0 centre, +5 right",
        type = control,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    pan: Option<PolySignal>,
    #[input(
        "how far the channels fan out around their pan positions: 0 none, 5 widest",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    width: Option<MonoSignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct StereoMixerOutputs {
    #[output("output", "stereo mix: left on channel 0, right on channel 1", type = cv, default)]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct ChannelState {
    pan: SwapGlide,
    /// The final pan the cached gains were computed for, and the
    /// `(left, right)` gains.
    gain_cache: Option<(f32, f32, f32)>,
}

const STEREO_MIX_DOCS: ModuleDocs = ModuleDocs {
    summary: "Stereo panner and spreader",
    description: "Places a signal in the stereo field and puts out left and right. Give each \
                  channel its own `pan`, or raise `width` to fan the voices of a chord out \
                  from the centre.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "Chord fanned across the stereo field",
        code: r#"
            $stereoMix($saw($chord('C3[m9]')).$.lpf('c5', 1), {
              width: 4,
            }).out()
        "#,
    }],
};

#[module(
    name = "$stereoMix",
    docs = STEREO_MIX_DOCS,
    channels = 2,
    args(input),
    has_init,
    block
)]
pub struct StereoMixer {
    outputs: StereoMixerOutputs,
    params: StereoMixerParams,
    state: StereoMixerState,
    /// Per-input-channel pan state. Sized to the *input* signal's channel
    /// count (not the module's fixed 2 output channels) in `init`, so its
    /// length is decoupled from the derived channel count. State transfer
    /// carries over the overlapping channels; a width change keeps the surplus
    /// freshly initialised.
    channel_state: Box<[ChannelState]>,
}

/// Module-level state for the StereoMixer.
#[derive(Default)]
struct StereoMixerState {
    /// Width buffer for stereo spread
    width_buffer: SwapGlide,
}

impl StereoMixer {
    fn init(&mut self, _sample_rate: f32) {
        // One pan glide per input channel.
        let input_channels = self.params.input.channels().max(1);
        self.channel_state = vec![ChannelState::default(); input_channels].into_boxed_slice();
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
            &mut self.channel_state[..p.input.channels()],
            width,
            |ch| p.input.get_value(ch),
            |ch, glide| {
                glide.follow(
                    p.pan.value_or_zero(ch).clamp(-5.0, 5.0),
                    p.pan.glide_mark(ch),
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
        let input_channels = p.input.channels();
        let width_glide = &mut self.state.width_buffer;
        let channel_state = &mut self.channel_state[..input_channels];
        let gliding = width_glide.will_glide(p.width.glide_mark())
            || channel_state
                .iter()
                .enumerate()
                .any(|(ch, state)| state.pan.will_glide(p.pan.glide_mark(ch)));
        if gliding {
            return None;
        }
        let mut scratch: [InputScratch; 3] = std::array::from_fn(|_| InputScratch::new());
        let [s0, s1, s2] = &mut scratch;
        let input = p.input.block_input(range.clone(), input_channels, s0)?;
        let pan = p
            .pan
            .block_input_or(range.clone(), input_channels, 0.0, s1)?;
        let width = match &p.width {
            Some(width) => {
                let width = width.poly();
                Some(width.block_input(range.clone(), width.channels(), s2)?)
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
                channel_state,
                width_at(i),
                |ch| input[ch],
                |ch, _| pan[ch].clamp(-5.0, 5.0),
            );
            let out = outputs.sample.slot_mut(i);
            out[0] = sanitize(left);
            out[1] = sanitize(right);
        }
        let last = range.end - 1;
        width_glide.settle(width_at(last), p.width.glide_mark());
        for (ch, (state, &pan)) in channel_state.iter_mut().zip(pan.row(last)).enumerate() {
            state.pan.settle(pan.clamp(-5.0, 5.0), p.pan.glide_mark(ch));
        }
        Some(())
    }
}

/// One sample of the stereo mix: each channel `input(ch)` placed at the base
/// pan `pan(ch, glide)` returns, spread by `width`, summed into `(left, right)`.
#[inline(always)]
fn mix_sample(
    channel_state: &mut [ChannelState],
    width: f32,
    input: impl Fn(usize) -> f32,
    mut pan: impl FnMut(usize, &mut SwapGlide) -> f32,
) -> (f32, f32) {
    let input_channels = channel_state.len();
    let mut left_sum = 0.0f32;
    let mut right_sum = 0.0f32;

    for (ch, state) in channel_state.iter_mut().enumerate() {
        let input = input(ch);
        let base_pan = pan(ch, &mut state.pan);

        // Voices spread from -width (voice 0) to +width (last voice) around
        // their base pan.
        let spread_offset = if input_channels > 1 {
            let voice_pos = ch as f32 / (input_channels - 1) as f32;
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

impl ChannelState {
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

    /// Build a StereoMixer the way the module macro would: fixed 2-channel
    /// stereo output, per-input-channel pan state sized by `init`. A fresh
    /// `SwapGlide` starts on its input, so a single call shows the full
    /// pan/width.
    fn make_stereo(input: PolySignal) -> StereoMixer {
        let mut outputs = StereoMixerOutputs::default();
        outputs.set_all_channels(2);
        let mut m = StereoMixer {
            outputs,
            params: StereoMixerParams {
                input,
                pan: None,
                width: None,
            },
            state: StereoMixerState::default(),
            channel_state: Box::default(),
            _channel_count: 2,
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
    fn pan_state_sized_to_input_channels() {
        assert_eq!(make_stereo(poly(5)).channel_state.len(), 5);
        assert_eq!(make_stereo(poly(1)).channel_state.len(), 1);
    }

    #[test]
    fn wide_input_does_not_panic_and_outputs_stereo() {
        let mut m = make_stereo(poly(40));
        m.update(48000.0);
        assert_eq!(m.outputs.sample.channels(), 2);
    }

    #[test]
    fn test_default_width_is_no_spread() {
        // With no width signal connected, width defaults to 0 (no spread):
        // every voice stays centered, so a 2-voice input sums equally to both
        // channels at the equal-power center gain sqrt(0.5). (A width of 5
        // would instead hard-pan voice 0 left and voice 1 right.)
        let mut m = make_stereo(PolySignal::poly(&[Signal::Volts(1.0), Signal::Volts(0.0)]));
        m.update(48000.0);
        let center = 0.5f32.sqrt();
        approx(m.outputs.sample.get(0), center); // left  = voice 0 centered
        approx(m.outputs.sample.get(1), center); // right = voice 0 centered
    }
}
