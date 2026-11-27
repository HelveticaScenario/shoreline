//! Soft clipper: linear below a knee, rounding smoothly into a ceiling, with a
//! tanh saturation toward the same ceiling beside it.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::OUTPUT_FULL_SCALE_VOLTS;
use crate::dsp::utils::halfband::{Halfband2xDown, Halfband2xUp};
use crate::dsp::utils::saturation::tanh_pade;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::reference_category::ReferenceCategory;
use crate::types::SwapGlide;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SoftClipParams {
    #[input("signal to clip", type = audio)]
    input: PolySignal,
    #[input(
        "the level the output never rises past",
        type = control,
        default = 20.0,
        default_doc = "the audio output's full scale, 20",
        range = (0.0, ..),
        control_range = (0.0, 20.0),
    )]
    #[deserr(default)]
    ceiling: Option<PolySignal>,
    #[input(
        "how gradually the signal rounds into the ceiling: 0 clips hard, 5 starts rounding from silence",
        type = control,
        default = 2.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    knee: Option<PolySignal>,
}

const _: () = assert!(SoftClipParams::CEILING_DEFAULT == OUTPUT_FULL_SCALE_VOLTS);

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SoftClipOutputs {
    #[output("clip", "the clipped signal", type = cv, default)]
    clip: PolyOutput,
    #[output(
        "tanh",
        "the signal rounded into the ceiling along a tanh curve, which eases in sooner and approaches it more gradually",
        type = cv,
    )]
    tanh: PolyOutput,
}

#[derive(Default, Clone, Copy, SeedSide)]
struct LaneState {
    ceiling: SwapGlide,
    knee: SwapGlide,
    up: Halfband2xUp,
    down: Halfband2xDown,
    tanh_down: Halfband2xDown,
}

/// Where the knee ends, in knee widths past the threshold: the cubic
/// `u − 4u³/27` reaches 1 there with zero slope.
const KNEE_SPAN: f32 = 1.5;

/// The linear threshold and knee width for a ceiling and a `knee` param.
#[inline]
fn knee_shape(ceiling: f32, knee: f32) -> (f32, f32) {
    let ceiling = ceiling.max(0.0);
    let width = ceiling * (knee * 0.2).clamp(0.0, 1.0);
    (ceiling - width, width)
}

/// `x` unchanged up to `threshold`, then a cubic knee `width` wide that meets
/// `threshold + width` (the ceiling) with matching slope at both ends.
#[inline(always)]
fn soft_clip(x: f32, threshold: f32, width: f32) -> f32 {
    let a = x.abs();
    if a <= threshold {
        return x;
    }
    let y = if a >= threshold + KNEE_SPAN * width {
        threshold + width
    } else {
        let u = (a - threshold) / width;
        threshold + width * (u - u * u * u * (4.0 / 27.0))
    };
    y.copysign(x)
}

/// `x` unchanged up to `threshold`, then a tanh knee `width` wide that leaves
/// it with unity slope and reaches `threshold + width` (the ceiling) three
/// widths past it. A `width` of 0 clips hard; with a `threshold` of 0 this is
/// `ceiling · tanh(x / ceiling)`.
#[inline(always)]
fn tanh_clip(x: f32, threshold: f32, width: f32) -> f32 {
    let a = x.abs();
    if a <= threshold {
        return x;
    }
    (threshold + width * tanh_pade((a - threshold) / width)).copysign(x)
}

const SOFT_CLIP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Clip peaks gently",
    description: "Leaves a signal untouched until it nears the ceiling, then rounds its peaks \
                  off so they never pass it. In `$setEndOfChainCb` it catches the whole \
                  mix's peaks before the output, as the ceiling defaults to the output's full \
                  scale; elsewhere it clips each lane on its own. `_tanh` rounds into the same \
                  ceiling along a tanh curve, saturating at every level when `knee` is 5.",
    categories: &[ReferenceCategory::Distortion, ReferenceCategory::Dynamics],
    sections: &[],
    examples: &[
        DocExample {
            title: "Catch the whole mix",
            code: r#"
                $setEndOfChainCb((mix) => mix.softClip())
                chord: $saw('c3:m9')
                bass: $saw('c2')
            "#,
        },
        DocExample {
            title: "Hard-edged bass",
            code: "bass: $sine('c2').mul(3).softClip(4, 0.5)",
        },
        DocExample {
            title: "Warm tanh drive",
            code: "lead: $saw('a3').lpf('a5').mul(2).softClip(5, 5)._tanh",
        },
    ],
};

#[module(name = "$softClip", docs = SOFT_CLIP_DOCS, args(input, ceiling, knee), experimental)]
pub struct SoftClip {
    outputs: SoftClipOutputs,
    lane_state: Box<[LaneState]>,
    params: SoftClipParams,
}

impl SoftClip {
    fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        let connected = self.connected();
        let (clip_read, tanh_read) = (connected.clip, connected.tanh);
        for lane in 0..self.lane_count() {
            let state = &mut self.lane_state[lane];
            state.ceiling.update(
                p.ceiling.value_or(lane, SoftClipParams::CEILING_DEFAULT),
                p.ceiling.glide_mark(lane),
                sample_rate,
            );
            state.knee.update(
                p.knee.value_or(lane, SoftClipParams::KNEE_DEFAULT),
                p.knee.glide_mark(lane),
                sample_rate,
            );
            if !(clip_read || tanh_read) {
                continue;
            }
            // 2× oversampled: the halfband pair is flat across the audio band,
            // so a signal below the threshold passes with its level intact. The
            // downsampler rings on the clipped edges, so its output is held to
            // the ceiling as well. A downsampler paused while its output goes
            // unread settles within a few samples of being read again.
            let ceiling = (*state.ceiling).max(0.0);
            let (threshold, width) = knee_shape(ceiling, *state.knee);
            let (even, odd) = state.up.process(p.input.get_value(lane));
            if clip_read {
                let y = state.down.process(
                    soft_clip(even, threshold, width),
                    soft_clip(odd, threshold, width),
                );
                self.outputs.clip.set(lane, y.clamp(-ceiling, ceiling));
            }
            if tanh_read {
                let y = state.tanh_down.process(
                    tanh_clip(even, threshold, width),
                    tanh_clip(odd, threshold, width),
                );
                self.outputs.tanh.set(lane, y.clamp(-ceiling, ceiling));
            }
        }
    }
}

message_handlers!(impl SoftClip {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, Signal};
    use std::f32::consts::{FRAC_1_SQRT_2, TAU};

    const SR: f32 = 48000.0;

    fn make(ceiling: f32, knee: f32) -> SoftClip {
        let mut outputs = SoftClipOutputs::default();
        outputs.set_all_lanes(1);
        SoftClip {
            params: SoftClipParams {
                input: PolySignal::mono(Signal::Volts(0.0)),
                ceiling: Some(PolySignal::mono(Signal::Volts(ceiling))),
                knee: Some(PolySignal::mono(Signal::Volts(knee))),
            },
            outputs,
            _lane_count: 1,
            _channels: 1,
            _block_index: Default::default(),
            _connected: Default::default(),
            lane_state: vec![LaneState::default(); 1].into_boxed_slice(),
        }
    }

    /// Output `(peak, rms)` for a sine of `amp` volts at `hz`, after the
    /// filters settle.
    fn sine_levels(clip: &mut SoftClip, amp: f32, hz: f32) -> (f32, f32) {
        let (mut peak, mut sum) = (0.0f32, 0.0f32);
        for n in 0..8800 {
            let x = amp * (TAU * hz * n as f32 / SR).sin();
            clip.params.input = PolySignal::mono(Signal::Volts(x));
            clip.update(SR);
            if n >= 4000 {
                let y = clip.outputs.clip.get(0);
                peak = peak.max(y.abs());
                sum += y * y;
            }
        }
        (peak, (sum / 4800.0).sqrt())
    }

    #[test]
    fn curve_is_continuous_monotonic_and_capped() {
        for knee in [0.0f32, 0.5, 2.0, 5.0] {
            let (t, w) = knee_shape(5.0, knee);
            let mut prev = soft_clip(-30.0, t, w);
            for i in -3000..=3000 {
                let x = i as f32 * 0.01;
                let y = soft_clip(x, t, w);
                assert!(y >= prev - 1e-6, "knee {knee}: not monotonic at {x}");
                assert!(
                    y.abs() <= 5.0 + 1e-6,
                    "knee {knee}: {y} past ceiling at {x}"
                );
                if knee > 0.0 {
                    assert!((y - prev).abs() <= 0.01 + 1e-5, "knee {knee}: jump at {x}");
                }
                prev = y;
            }
            assert_eq!(soft_clip(30.0, t, w), 5.0);
        }
    }

    #[test]
    fn unity_below_threshold() {
        let (t, w) = knee_shape(5.0, 2.0);
        assert_eq!(t, 3.0);
        for x in [-3.0f32, -1.2, 0.0, 0.7, 3.0] {
            assert_eq!(soft_clip(x, t, w), x);
        }
    }

    #[test]
    fn quiet_signal_keeps_its_level_across_the_band() {
        for hz in [100.0f32, 1000.0, 10000.0, 16000.0] {
            let (_, rms) = sine_levels(&mut make(5.0, 2.0), 2.0, hz);
            assert!(
                (rms - 2.0 * FRAC_1_SQRT_2).abs() < 0.01,
                "{hz} Hz: rms {rms}"
            );
        }
    }

    #[test]
    fn loud_signal_is_held_near_the_ceiling() {
        for knee in [0.0f32, 2.0, 5.0] {
            let (peak, _) = sine_levels(&mut make(4.0, knee), 20.0, 100.0);
            assert!((peak - 4.0).abs() < 0.3, "knee {knee}: peak {peak}");
        }
    }

    #[test]
    fn tanh_curve_is_continuous_monotonic_and_capped() {
        for knee in [0.0f32, 0.5, 2.0, 5.0] {
            let (t, w) = knee_shape(5.0, knee);
            let mut prev = tanh_clip(-30.0, t, w);
            for i in -3000..=3000 {
                let x = i as f32 * 0.01;
                let y = tanh_clip(x, t, w);
                assert!(y >= prev - 1e-6, "knee {knee}: not monotonic at {x}");
                assert!(
                    y.abs() <= 5.0 + 1e-6,
                    "knee {knee}: {y} past ceiling at {x}"
                );
                if knee > 0.0 {
                    assert!((y - prev).abs() <= 0.01 + 1e-5, "knee {knee}: jump at {x}");
                }
                prev = y;
            }
            assert_eq!(tanh_clip(30.0, t, w), 5.0);
        }
    }

    #[test]
    fn tanh_knee_matches_the_clip_threshold_and_full_knee_is_plain_tanh() {
        let (t, w) = knee_shape(5.0, 2.0);
        for x in [-3.0f32, -1.2, 0.0, 0.7, 3.0] {
            assert_eq!(tanh_clip(x, t, w), x);
        }
        let (t, w) = knee_shape(4.0, 5.0);
        for x in [-9.0f32, -2.0, 0.01, 1.0, 3.5, 12.0] {
            assert!((tanh_clip(x, t, w) - 4.0 * tanh_pade(x / 4.0)).abs() < 1e-6);
        }
        let (t, w) = knee_shape(0.0, 2.0);
        assert_eq!(tanh_clip(3.0, t, w), 0.0);
    }

    #[test]
    fn hot_saw_never_passes_the_ceiling() {
        for knee in [0.0f32, 2.0, 5.0] {
            for hz in [55.0f32, 440.0, 3000.0] {
                let mut clip = make(20.0, knee);
                let (mut lo, mut hi) = ([0.0f32; 2], [0.0f32; 2]);
                for n in 0..48000 {
                    let phase = (hz * n as f32 / SR).fract();
                    clip.params.input = PolySignal::mono(Signal::Volts(25.0 * (2.0 * phase - 1.0)));
                    clip.update(SR);
                    for (i, y) in [clip.outputs.clip.get(0), clip.outputs.tanh.get(0)]
                        .into_iter()
                        .enumerate()
                    {
                        lo[i] = lo[i].min(y);
                        hi[i] = hi[i].max(y);
                    }
                }
                for i in 0..2 {
                    assert!(
                        lo[i] >= -20.0 && hi[i] <= 20.0,
                        "knee {knee} {hz} Hz output {i}: {}..{}",
                        lo[i],
                        hi[i]
                    );
                }
            }
        }
    }

    #[test]
    fn steady_level_settles_to_the_ceiling() {
        let mut clip = make(3.0, 2.0);
        clip.params.input = PolySignal::mono(Signal::Volts(-12.0));
        for _ in 0..2000 {
            clip.update(SR);
        }
        assert!((clip.outputs.clip.get(0) + 3.0).abs() < 1e-3);
        assert!((clip.outputs.tanh.get(0) + 3.0).abs() < 1e-3);
    }
}
