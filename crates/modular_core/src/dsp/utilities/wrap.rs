use deserr::Deserr;
use schemars::JsonSchema;

use crate::bounds::Bounds;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::utils::{Sample, wrap},
    poly::{PolyOutput, PolySignal},
};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct WrapParams {
    #[input("signal to wrap", type = audio)]
    input: PolySignal,
    #[input("bottom of the range; swaps with max when above it", type = control, default = 0.0, control_range = (-5.0, 5.0))]
    min: PolySignal,
    #[input("top of the range", type = control, default = 5.0, control_range = (-5.0, 5.0))]
    max: PolySignal,
    #[input("shift by whole volts (octaves) instead, keeping the note and moving only its octave")]
    #[serde(default)]
    #[deserr(default)]
    octave: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct WrapOutputs {
    #[output("output", "the wrapped signal", type = cv, precision = inherit, bounds = params, default)]
    sample: PolyOutput,
}

const WRAP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Wrap a signal into a range",
    description: "Wraps anything past one edge of a range back in from the other, the \
                  way a phase wraps around. In octave mode it moves notes by whole \
                  octaves instead, keeping a melody's notes but folding it into one \
                  register.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Wide melody folded into one octave",
        code: r#"
            $sine(
              $wrap($p('c2 g5 e3 b4 d6'), 'c4', 'c5', {
                octave: true,
              }),
            ).out()
        "#,
    }],
};

#[module(name = "$wrap", docs = WRAP_DOCS, args(input, min, max))]
pub struct Wrap {
    outputs: WrapOutputs,
    params: WrapParams,
}

// Closed [lo, hi] variant — same as wrap(lo..hi) except val == hi returns hi.
fn wrap_closed<T: Sample>(val: T, lo: T, hi: T) -> T {
    if val == hi { hi } else { wrap(lo..hi, val) }
}

fn wrap_octave<T: Sample>(wrapped: T, x: T) -> T {
    let f = x - x.floor();
    f + (wrapped - f).round()
}

/// `val` wrapped into the range between `a` and `b`, by whole octaves when
/// `octave` is set.
fn wrap_value<T: Sample>(val: T, a: T, b: T, octave: bool) -> T {
    let (min, max) = if b < a { (b, a) } else { (a, b) };
    if (max - min).abs() < T::EPSILON {
        min
    } else if octave {
        wrap_octave(wrap_closed(val, min, max), val)
    } else {
        wrap_closed(val, min, max)
    }
}

/// The span a wrapped signal stays in: the range, rising, or in octave mode
/// half a volt past either end, where a note keeps its pitch class.
fn wrapped_bounds(a: f32, b: f32, octave: bool) -> Bounds {
    let span = Bounds::new(a, b).sorted();
    if octave {
        Bounds::new(span.start - 0.5, span.end + 0.5)
    } else {
        span
    }
}

impl Wrap {
    fn update(&mut self, _sample_rate: f32) {
        let (params, octave) = (&self.params, self.params.octave);
        if self.outputs.sample.is_bounded() {
            for i in 0..self.channel_count() {
                let bounds =
                    wrapped_bounds(params.min.get_value(i), params.max.get_value(i), octave);
                self.outputs.sample.set_bounds(i, bounds);
            }
        }
        if self.outputs.sample.is_f64() {
            for i in 0..self.channel_count() {
                let output = wrap_value(
                    params.input.get_value_f64(i),
                    params.min.get_value_f64(i),
                    params.max.get_value_f64(i),
                    octave,
                );
                self.outputs.sample.set_f64(i, output);
            }
            return;
        }
        for i in 0..self.channel_count() {
            let output = wrap_value(
                params.input.get_value(i),
                params.min.get_value(i),
                params.max.get_value(i),
                octave,
            );
            self.outputs.sample.set(i, output);
        }
    }
}

message_handlers!(impl Wrap {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{poly::PolySignal, types::Signal};

    fn run_wrap(input: f32, min: f32, max: f32) -> f32 {
        let mut module = Wrap {
            outputs: WrapOutputs::default(),
            params: WrapParams {
                input: PolySignal::mono(Signal::Volts(input)),
                min: PolySignal::mono(Signal::Volts(min)),
                max: PolySignal::mono(Signal::Volts(max)),
                octave: false,
            },
            _channel_count: 1,
            _block_index: Default::default(),
        };
        module.outputs.sample.set_channels(1);
        module.update(44100.0);
        module.outputs.sample.get(0)
    }

    fn run_wrap_octave(input: f32, min: f32, max: f32) -> f32 {
        let mut module = Wrap {
            outputs: WrapOutputs::default(),
            params: WrapParams {
                input: PolySignal::mono(Signal::Volts(input)),
                min: PolySignal::mono(Signal::Volts(min)),
                max: PolySignal::mono(Signal::Volts(max)),
                octave: true,
            },
            _channel_count: 1,
            _block_index: Default::default(),
        };
        module.outputs.sample.set_channels(1);
        module.update(44100.0);
        module.outputs.sample.get(0)
    }

    #[test]
    fn wrap_within_range_unchanged() {
        let result = run_wrap(2.5, 0.0, 5.0);
        assert!((result - 2.5).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn wrap_above_max_folds_back() {
        // 6.0 in [0, 5) → 1.0
        let result = run_wrap(6.0, 0.0, 5.0);
        assert!((result - 1.0).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn wrap_below_min_folds_forward() {
        // -1.0 in [0, 5) → 4.0
        let result = run_wrap(-1.0, 0.0, 5.0);
        assert!((result - 4.0).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn wrap_exactly_at_max_stays() {
        // 5.0 in [0, 5] → 5.0 (inclusive upper bound)
        let result = run_wrap(5.0, 0.0, 5.0);
        assert!((result - 5.0).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn wrap_exactly_at_min_stays() {
        let result = run_wrap(0.0, 0.0, 5.0);
        assert!((result - 0.0).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn wrap_swaps_when_max_less_than_min() {
        // max=0, min=5 → treated as [0, 5); 6 → 1
        let result = run_wrap(6.0, 5.0, 0.0);
        assert!((result - 1.0).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn wrap_degenerate_zero_width_outputs_min() {
        let result = run_wrap(3.0, 2.0, 2.0);
        assert!((result - 2.0).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn wrap_negative_range() {
        // 0.5 in [-1, 1) → 0.5
        let result = run_wrap(0.5, -1.0, 1.0);
        assert!((result - 0.5).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn wrap_far_above_range_multiple_cycles() {
        // 11.0 in [0, 5) → 1.0 (two full cycles above)
        let result = run_wrap(11.0, 0.0, 5.0);
        assert!((result - 1.0).abs() < 1e-5, "got {result}");
    }

    // octave mode

    #[test]
    fn octave_within_range_unchanged() {
        // 1.5 already in [0, 2) — no shift needed
        let result = run_wrap_octave(1.5, 0.0, 2.0);
        assert!((result - 1.5).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn octave_shifts_down_to_fit() {
        // 3.5 → pitch class 0.5, window [0, 2): closest valid octave is 1.5
        let result = run_wrap_octave(3.5, 0.0, 2.0);
        assert!((result - 1.5).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn octave_shifts_up_to_fit() {
        // -1.5 → pitch class 0.5, window [0, 2): shifts to 0.5
        let result = run_wrap_octave(-1.5, 0.0, 2.0);
        assert!((result - 0.5).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn octave_preserves_frac_across_many_octaves() {
        // 7.3 → pitch class ~0.3, window [0, 2): n_ideal=7 clamped to n_max=1, output ~1.3
        let result = run_wrap_octave(7.3, 0.0, 2.0);
        assert!((result - 1.3).abs() < 1e-4, "got {result}");
    }

    #[test]
    fn octave_negative_input_frac_positive() {
        // -0.3 wraps to 1.7 in [0, 2); frac of -0.3 is 0.7, frac of 1.7 is 0.7 — already matches
        let result = run_wrap_octave(-0.3, 0.0, 2.0);
        assert!((result - 1.7).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn octave_impossible_best_attempt_above() {
        // frac 0.9, window [0.2, 0.7): wraps to ~0.4; nearest same-frac to 0.4 is -0.1 or 0.9 (equidistant); f32 rounds to -0.1
        let result = run_wrap_octave(1.9, 0.2, 0.7);
        assert!((result - (-0.1)).abs() < 1e-5, "got {result}");
    }

    #[test]
    fn octave_impossible_best_attempt_below() {
        // frac 0.1, window [0.3, 0.8): wraps to ~0.6; nearest same-frac to 0.6 is 1.1 (dist 0.5) vs -0.9 (dist 1.5) → 1.1
        let result = run_wrap_octave(1.1, 0.3, 0.8);
        assert!((result - 1.1).abs() < 1e-4, "got {result}");
    }

    #[test]
    fn octave_span_exactly_one_always_fits() {
        // window [0.3, 1.3): span=1, frac 0.7 → 0.7; frac 0.1 → 0.1+1=1.1; frac 0.3 → 0.3
        assert!((run_wrap_octave(0.7, 0.3, 1.3) - 0.7).abs() < 1e-5);
        assert!((run_wrap_octave(2.1, 0.3, 1.3) - 1.1).abs() < 1e-4);
        assert!((run_wrap_octave(5.3, 0.3, 1.3) - 0.3).abs() < 1e-4);
    }
}
