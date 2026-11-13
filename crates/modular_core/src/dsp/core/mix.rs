use deserr::Deserr;
use schemars::JsonSchema;

use std::mem::MaybeUninit;
use std::ops::Range;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    block_input::{InputScratch, OptionBlockInput, SCRATCH_LEN},
    dsp::utils::{audio_taper, sanitize},
    poly::{PORT_MAX_LANES, PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};

/// Mixing mode for combining input signals.
#[derive(Clone, Copy, Debug, Default, Deserr, JsonSchema, PartialEq, Eq, Connect)]
#[serde(rename_all = "snake_case")]
#[deserr(rename_all = lowercase)]
pub enum MixMode {
    /// every signal added together, getting louder as more stack up
    #[default]
    Sum,
    /// the average of the signals, staying at one signal's level however many stack up
    Average,
    /// only the loudest signal at each moment
    Max,
    /// only the quietest signal that isn't silent at each moment
    Min,
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
pub struct MixParams {
    #[input("the signals to mix; lane n of each input mixes into output lane n")]
    pub inputs: Vec<PolySignal>,
    #[input("how the inputs combine", default = "sum")]
    #[serde(default)]
    #[deserr(default)]
    pub mode: MixMode,
    #[input(
        "output level on a cubic curve: 0 silent, 5 unity, 10 eight times louder; negative flips polarity",
        type = control,
        default = 5.0,
        control_range = (0.0, 10.0),
    )]
    #[deserr(default)]
    pub gain: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MixOutputs {
    #[output(
        "output",
        "the mix, keeping each lane separate rather than folding to mono",
        type = cv,
        default
    )]
    sample: PolyOutput,
}

/// Custom lane count derivation for Mix.
///
/// Mix output lanes = max(max_input_lanes, gain_lanes), at least 1.
/// This matches the runtime behavior in update().
pub fn mix_derive_lane_count(params: &MixParams) -> usize {
    // Get max lane count from inputs
    let input_refs: Vec<&PolySignal> = params.inputs.iter().collect();

    let max_input_lanes = if params.inputs.is_empty() {
        0usize
    } else {
        PolySignal::max_lanes(&input_refs)
    };

    // Get gain lane count
    let gain_lanes = params.gain.lane_count();

    // Output lanes = max(max_input_lanes, gain_lanes), at least 1 if inputs empty
    if params.inputs.is_empty() {
        gain_lanes.max(1)
    } else {
        max_input_lanes.max(gain_lanes)
    }
    .min(PORT_MAX_LANES)
}

const MIX_DOCS: ModuleDocs = ModuleDocs {
    summary: "Lane-by-lane mixer",
    description: "Mixes several signals into one, keeping their lanes apart: lane n of \
                  every input lands on output lane n, so mixing two four-voice signals \
                  still gives four voices. Use `$setLanes` to fold lanes together instead.",
    categories: &[ModuleCategory::Mixer],
    sections: &[],
    examples: &[DocExample {
        title: "Saw and sub-octave square layered",
        code: r#"
            $mix([$saw('c3'), $pulse('c2')], { gain: 3 })
              .lpf('c5', 1)
              .out()
        "#,
    }],
};

#[module(name = "$mix", docs = MIX_DOCS, lanes_derive = mix_derive_lane_count, args(inputs), block)]
pub struct Mix {
    outputs: MixOutputs,
    params: MixParams,
    lane_state: Box<[SwapGlide]>,
}

message_handlers!(impl Mix {});

/// Max mode: keep the value of largest magnitude. NaN comparisons are false,
/// so NaN never replaces a finite best.
#[inline(always)]
fn fold_max(best_abs: &mut f32, best_val: &mut f32, v: f32) {
    if v.abs() > *best_abs {
        *best_abs = v.abs();
        *best_val = v;
    }
}

/// Max mode's result from a fold seeded with `best_abs = -1`.
#[inline(always)]
fn max_result(best_abs: f32, best_val: f32) -> f32 {
    if best_abs >= 0.0 { best_val } else { 0.0 }
}

/// Min mode: keep the nonzero value of smallest magnitude.
#[inline(always)]
fn fold_min(best_abs: &mut f32, best_val: &mut f32, v: f32) {
    if v != 0.0 && v.abs() < *best_abs {
        *best_abs = v.abs();
        *best_val = v;
    }
}

/// Min mode's result from a fold seeded with `best_abs = inf`.
#[inline(always)]
fn min_result(best_abs: f32, best_val: f32) -> f32 {
    if best_abs.is_finite() { best_val } else { 0.0 }
}

/// Initialise `len` values at `buffer` to `value` and borrow them.
fn init_slice(buffer: &mut MaybeUninit<[f32; SCRATCH_LEN]>, len: usize, value: f32) -> &mut [f32] {
    let data = buffer.as_mut_ptr() as *mut f32;
    // SAFETY: `len <= SCRATCH_LEN`, and every borrowed value is written first.
    unsafe {
        for k in 0..len {
            data.add(k).write(value);
        }
        std::slice::from_raw_parts_mut(data, len)
    }
}

/// Call `f(n, row)` with each input's row for the `n`th slot of `range`, input
/// by input. A row holds just that input's lanes.
#[inline(always)]
fn for_each_input_row(
    inputs: &[PolySignal],
    range: Range<usize>,
    mut f: impl FnMut(usize, &[f32]),
) -> Option<()> {
    for input in inputs {
        let mut scratch = InputScratch::new();
        let rows = input.block_input(range.clone(), input.lanes(), &mut scratch)?;
        for (n, i) in range.clone().enumerate() {
            f(n, rows.row(i));
        }
    }
    Some(())
}

#[doc(hidden)]
pub fn __bench_make_mix(params: MixParams) -> Mix {
    use crate::types::OutputStruct;
    let lanes = mix_derive_lane_count(&params);
    let mut outputs = MixOutputs::default();
    outputs.set_all_lanes(lanes);
    Mix {
        params,
        outputs,
        _lane_count: lanes,
        _block_index: Default::default(),
        lane_state: vec![SwapGlide::default(); lanes].into_boxed_slice(),
    }
}

impl Mix {
    #[doc(hidden)]
    pub fn __bench_update(&mut self, sample_rate: f32) {
        self.update(sample_rate);
    }

    fn update(&mut self, sample_rate: f32) {
        let inputs = &self.params.inputs;
        let gain = &self.params.gain;

        let output_lanes = self.lane_count();

        // Empty inputs case — output silence.
        if inputs.is_empty() {
            for i in 0..output_lanes {
                self.outputs.sample.set(i, 0.0);
            }
            return;
        }

        // Max lane count over all inputs.
        let mut max_input_lanes: usize = 0;
        for input in inputs.iter() {
            let c = input.lanes();
            if c > max_input_lanes {
                max_input_lanes = c;
            }
        }

        // Pre-compute mixed values per lane. The mode match is hoisted
        // out of the per-lane loop so we don't compute sum + max-by-abs
        // + min-by-abs accumulators on every iteration when only one is
        // selected. Each branch's inner loop carries only the state its
        // mode needs.
        let mut pre_gain_values = [0.0f32; PORT_MAX_LANES];
        match self.params.mode {
            MixMode::Sum => {
                for (lane, value) in pre_gain_values[..max_input_lanes].iter_mut().enumerate() {
                    let mut sum: f32 = 0.0;
                    for input in inputs.iter() {
                        if lane >= input.lanes() {
                            continue;
                        }
                        sum += input.get_value(lane);
                    }
                    *value = sum;
                }
            }
            MixMode::Average => {
                for (lane, value) in pre_gain_values[..max_input_lanes].iter_mut().enumerate() {
                    let mut sum: f32 = 0.0;
                    let mut count: u32 = 0;
                    for input in inputs.iter() {
                        if lane >= input.lanes() {
                            continue;
                        }
                        sum += input.get_value(lane);
                        count += 1;
                    }
                    *value = if count > 0 { sum / count as f32 } else { 0.0 };
                }
            }
            MixMode::Max => {
                for (lane, value) in pre_gain_values[..max_input_lanes].iter_mut().enumerate() {
                    let mut best_abs: f32 = -1.0;
                    let mut best_val: f32 = 0.0;
                    for input in inputs.iter() {
                        if lane >= input.lanes() {
                            continue;
                        }
                        fold_max(&mut best_abs, &mut best_val, input.get_value(lane));
                    }
                    *value = max_result(best_abs, best_val);
                }
            }
            MixMode::Min => {
                for (lane, value) in pre_gain_values[..max_input_lanes].iter_mut().enumerate() {
                    let mut best_abs: f32 = f32::INFINITY;
                    let mut best_val: f32 = 0.0;
                    for input in inputs.iter() {
                        if lane >= input.lanes() {
                            continue;
                        }
                        fold_min(&mut best_abs, &mut best_val, input.get_value(lane));
                    }
                    *value = min_result(best_abs, best_val);
                }
            }
        }

        // Apply gain with cycling on pre_gain_values. Outside a glide the gain
        // reads raw, exactly as `process_block` does.
        for i in 0..output_lanes {
            let pre_gain_index = i % max_input_lanes;
            let pre_gain_value = pre_gain_values[pre_gain_index];
            let gain_value = self.lane_state[i].follow(
                audio_taper(gain.value_or(i, 5.0)),
                gain.glide_mark(i),
                sample_rate,
            );
            self.outputs.sample.set(i, pre_gain_value * gain_value);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut MixBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let output_lanes = self.lane_count();
        let inputs = &self.params.inputs;
        let gain_param = &self.params.gain;
        let glides = &mut self.lane_state[..output_lanes];
        let out = &mut outputs.sample;
        if glides
            .iter()
            .enumerate()
            .any(|(i, glide)| glide.will_glide(gain_param.glide_mark(i)))
        {
            return None;
        }
        if inputs.is_empty() {
            for i in range {
                out.slot_mut(i).fill(0.0);
            }
            return Some(());
        }
        // Inputs fold into per-slot values over the widest input's lanes;
        // a narrower input leaves its missing lanes out.
        let width = inputs.iter().map(PolySignal::lanes).max().unwrap_or(1);
        let len = range.len() * width;
        if len > SCRATCH_LEN {
            return None;
        }
        let mut gain_scratch = InputScratch::new();
        let gain =
            gain_param.block_input_or(range.clone(), output_lanes, 5.0, &mut gain_scratch)?;

        // `pre` ends up holding each slot's pre-gain value per lane.
        let (mut pre_buffer, mut best_buffer) = (MaybeUninit::uninit(), MaybeUninit::uninit());
        let pre = init_slice(&mut pre_buffer, len, 0.0);
        match self.params.mode {
            MixMode::Sum => {
                for_each_input_row(inputs, range.clone(), |n, row| {
                    for (sum, &v) in pre[n * width..].iter_mut().zip(row) {
                        *sum += v;
                    }
                })?;
            }
            MixMode::Average => {
                let mut counts = [0u32; PORT_MAX_LANES];
                for input in inputs.iter() {
                    for count in &mut counts[..input.lanes()] {
                        *count += 1;
                    }
                }
                for_each_input_row(inputs, range.clone(), |n, row| {
                    for (sum, &v) in pre[n * width..].iter_mut().zip(row) {
                        *sum += v;
                    }
                })?;
                for slot in pre.chunks_exact_mut(width) {
                    for (value, &count) in slot.iter_mut().zip(&counts) {
                        *value = if count > 0 {
                            *value / count as f32
                        } else {
                            0.0
                        };
                    }
                }
            }
            MixMode::Max => {
                let best = init_slice(&mut best_buffer, len, -1.0);
                for_each_input_row(inputs, range.clone(), |n, row| {
                    let slots = pre[n * width..].iter_mut().zip(&mut best[n * width..]);
                    for ((value, best_abs), &v) in slots.zip(row) {
                        fold_max(best_abs, value, v);
                    }
                })?;
                for (value, &best_abs) in pre.iter_mut().zip(best.iter()) {
                    *value = max_result(best_abs, *value);
                }
            }
            MixMode::Min => {
                let best = init_slice(&mut best_buffer, len, f32::INFINITY);
                for_each_input_row(inputs, range.clone(), |n, row| {
                    let slots = pre[n * width..].iter_mut().zip(&mut best[n * width..]);
                    for ((value, best_abs), &v) in slots.zip(row) {
                        fold_min(best_abs, value, v);
                    }
                })?;
                for (value, &best_abs) in pre.iter_mut().zip(best.iter()) {
                    *value = min_result(best_abs, *value);
                }
            }
        }

        for (n, i) in range.clone().enumerate() {
            let pre = &pre[n * width..(n + 1) * width];
            let gains = gain.row(i);
            for (lane, value) in out.slot_mut(i).iter_mut().enumerate() {
                *value = sanitize(pre[lane % width] * audio_taper(gains[lane]));
            }
        }
        let last = gain.row(range.end - 1);
        for (i, glide) in glides.iter_mut().enumerate() {
            glide.settle(audio_taper(last[i]), gain_param.glide_mark(i));
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly::PolySignal;
    use crate::types::{OutputStruct, Signal};

    /// Create a Mix with params and properly initialize _lane_count and output lanes.
    fn make_mix(params: MixParams) -> Mix {
        let lanes = mix_derive_lane_count(&params);
        let mut outputs = MixOutputs::default();
        outputs.set_all_lanes(lanes);

        Mix {
            params,
            outputs,
            _lane_count: lanes,
            _block_index: Default::default(),
            lane_state: vec![SwapGlide::default(); lanes].into_boxed_slice(),
        }
    }

    #[test]
    fn test_mix_single_poly_sum() {
        let mut mixer = make_mix(MixParams {
            inputs: vec![PolySignal::poly(&[
                Signal::Volts(1.0),
                Signal::Volts(2.0),
                Signal::Volts(3.0),
            ])],
            mode: MixMode::Sum,
            gain: None,
        });
        mixer.update(48000.0);
        assert_eq!(mixer.outputs.sample.lanes(), 3);
        assert_eq!(mixer.outputs.sample.get(0), 1.0);
        assert_eq!(mixer.outputs.sample.get(1), 2.0);
        assert_eq!(mixer.outputs.sample.get(2), 3.0);
    }

    #[test]
    fn test_mix_two_poly_sum() {
        // A: 2 lanes [1, 2], B: 3 lanes [10, 20, 30]
        let mut mixer = make_mix(MixParams {
            inputs: vec![
                PolySignal::poly(&[Signal::Volts(1.0), Signal::Volts(2.0)]),
                PolySignal::poly(&[
                    Signal::Volts(10.0),
                    Signal::Volts(20.0),
                    Signal::Volts(30.0),
                ]),
            ],
            mode: MixMode::Sum,
            gain: None,
        });
        mixer.update(48000.0);
        // Output should be 3 lanes
        assert_eq!(mixer.outputs.sample.lanes(), 3);
        // Lane 0: 1 + 10 = 11
        assert_eq!(mixer.outputs.sample.get(0), 11.0);
        // Lane 1: 2 + 20 = 22
        assert_eq!(mixer.outputs.sample.get(1), 22.0);
        // Lane 2: 0 + 30 = 30 (A has no lane 2, contributes 0)
        assert_eq!(mixer.outputs.sample.get(2), 30.0);
    }

    #[test]
    fn test_mix_average_mode() {
        // A: 2 lanes [2, 4], B: 2 lanes [6, 8]
        let mut mixer = make_mix(MixParams {
            inputs: vec![
                PolySignal::poly(&[Signal::Volts(2.0), Signal::Volts(4.0)]),
                PolySignal::poly(&[Signal::Volts(6.0), Signal::Volts(8.0)]),
            ],
            mode: MixMode::Average,
            gain: None,
        });
        mixer.update(48000.0);
        assert_eq!(mixer.outputs.sample.lanes(), 2);
        // Lane 0: (2 + 6) / 2 = 4
        assert_eq!(mixer.outputs.sample.get(0), 4.0);
        // Lane 1: (4 + 8) / 2 = 6
        assert_eq!(mixer.outputs.sample.get(1), 6.0);
    }

    #[test]
    fn test_mix_gain_extends_lanes() {
        // A: 1 lane [5], B: 2 lanes [10, 20], gain: 3 lanes [1, 2, 0.5]
        let mut mixer = make_mix(MixParams {
            inputs: vec![
                PolySignal::mono(Signal::Volts(5.0)),
                PolySignal::poly(&[Signal::Volts(10.0), Signal::Volts(20.0)]),
            ],
            mode: MixMode::Sum,
            gain: Some(PolySignal::poly(&[
                Signal::Volts(5.0),
                Signal::Volts(10.0),
                Signal::Volts(2.5),
            ])),
        });
        mixer.update(48000.0);
        // Output lanes = max(2 input lanes, 3 gain lanes) = 3
        assert_eq!(mixer.outputs.sample.lanes(), 3);
        // Lane 0: (5 + 10) * (5/5)^3 = 15 * 1.0 = 15
        assert_eq!(mixer.outputs.sample.get(0), 15.0);
        // Lane 1: (0 + 20) * (10/5)^3 = 20 * 8.0 = 160
        assert_eq!(mixer.outputs.sample.get(1), 160.0);
        // Lane 2: pre_gain cycles from lane 0 (15 pre-gain), gain[2] = (2.5/5)^3 = 0.125 -> 15 * 0.125 = 1.875
        assert_eq!(mixer.outputs.sample.get(2), 1.875);
    }

    #[test]
    fn test_mix_empty_inputs() {
        let mut mixer = make_mix(MixParams {
            inputs: vec![],
            mode: MixMode::Sum,
            gain: Some(PolySignal::poly(&[
                Signal::Volts(1.0),
                Signal::Volts(2.0),
                Signal::Volts(3.0),
            ])),
        });
        mixer.update(48000.0);
        // Empty inputs with 3-lane gain -> 3 lanes of silence
        assert_eq!(mixer.outputs.sample.lanes(), 3);
        assert_eq!(mixer.outputs.sample.get(0), 0.0);
        assert_eq!(mixer.outputs.sample.get(1), 0.0);
        assert_eq!(mixer.outputs.sample.get(2), 0.0);
    }

    #[test]
    fn test_mix_empty_inputs_no_gain() {
        let params: MixParams =
            deserr::deserialize::<MixParams, _, crate::param_errors::ModuleParamErrors>(
                serde_json::json!({"inputs": []}),
            )
            .unwrap();
        let mut mixer = make_mix(params);
        mixer.update(48000.0);
        // Empty inputs with no gain -> 1 lane of silence
        assert_eq!(mixer.outputs.sample.lanes(), 1);
        assert_eq!(mixer.outputs.sample.get(0), 0.0);
    }

    #[test]
    fn test_mix_max_mode() {
        let mut mixer = make_mix(MixParams {
            inputs: vec![
                PolySignal::poly(&[Signal::Volts(1.0), Signal::Volts(-5.0)]),
                PolySignal::poly(&[Signal::Volts(-3.0), Signal::Volts(2.0)]),
            ],
            mode: MixMode::Max,
            gain: None,
        });
        mixer.update(48000.0);
        assert_eq!(mixer.outputs.sample.lanes(), 2);
        // Lane 0: max by abs(1, -3) = -3
        assert_eq!(mixer.outputs.sample.get(0), -3.0);
        // Lane 1: max by abs(-5, 2) = -5
        assert_eq!(mixer.outputs.sample.get(1), -5.0);
    }

    #[test]
    fn test_mix_nan_input_does_not_panic() {
        for mode in [MixMode::Max, MixMode::Min] {
            let mut mixer = make_mix(MixParams {
                inputs: vec![
                    PolySignal::poly(&[Signal::Volts(f32::NAN)]),
                    PolySignal::poly(&[Signal::Volts(1.0)]),
                ],
                mode,
                gain: None,
            });
            mixer.update(48000.0);
            // Must not panic; output may be NaN or a finite value — either is acceptable
            let _ = mixer.outputs.sample.get(0);
        }
    }
}
