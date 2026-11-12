//! Rounding modules: `$round`, `$floor` and `$ceil` take a signal to whole
//! volts, which on a pitch means whole octaves. Each is stamped by
//! [`rounding_module!`] from its rounding function; the update and block
//! paths here are shared by all three, and so is the rule for the output's
//! bounds: rounding is monotonic, so the bounds are the input's rounded.

use std::ops::Range;

use crate::block_input::{self, InputScratch};
use crate::block_port::BlockPort;
use crate::dsp::utils::Sample;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};

/// How a module rounds to a whole number.
trait Rounding {
    fn apply<T: Sample>(x: T) -> T;
}

/// One sample of every channel, at full precision when `out` is f64.
#[inline(always)]
fn update<R: Rounding>(input: &PolySignal, out: &mut PolyOutput, channels: usize) {
    if out.is_f64() {
        for i in 0..channels {
            out.set_f64(i, R::apply(input.get_value_f64(i)));
        }
    } else {
        for i in 0..channels {
            out.set(i, R::apply(input.get_value(i)));
        }
    }
    if out.stores_bounds() {
        for i in 0..channels {
            out.set_bounds(i, input.get_bounds(i).map(R::apply));
        }
    }
}

#[inline(always)]
fn process_block<R: Rounding>(
    input: &PolySignal,
    out: &mut BlockPort,
    range: Range<usize>,
    channels: usize,
) -> Option<()> {
    let mut scratch = InputScratch::new();
    let samples = input.block_input(range.clone(), channels, &mut scratch)?;
    out.fill_bounds_with(range.clone(), |ch| {
        Some(input.block_bounds(range.clone(), ch)?.map(R::apply))
    })?;
    block_input::map(out, range, [samples], |[x]| R::apply(x));
    Some(())
}

/// Stamp a rounding module: one input, rounded by `|x| rounding` at either
/// precision, and one output at the precision of its input.
macro_rules! rounding_module {
    (
        $Module:ident,
        name = $name:literal,
        docs = $docs:path,
        |$x:ident| $rounding:expr $(,)?
    ) => {
        paste::paste! {
            #[params]
            #[derive(Clone, deserr::Deserr, schemars::JsonSchema, Connect, ChannelCount)]
            #[serde(rename_all = "camelCase")]
            #[deserr(rename_all = camelCase, deny_unknown_fields)]
            struct [<$Module Params>] {
                #[input("signal to round", type = audio, bounds)]
                input: PolySignal,
            }

            #[derive(Outputs, schemars::JsonSchema)]
            #[serde(rename_all = "camelCase")]
            struct [<$Module Outputs>] {
                #[output("output", "the signal in whole volts", type = cv, precision = inherit, bounds = input, default)]
                sample: PolyOutput,
            }

            #[module(name = $name, docs = $docs, args(input), block)]
            pub struct $Module {
                outputs: [<$Module Outputs>],
                params: [<$Module Params>],
            }

            struct [<$Module Rounding>];

            impl Rounding for [<$Module Rounding>] {
                #[inline(always)]
                fn apply<T: Sample>($x: T) -> T {
                    $rounding
                }
            }

            impl $Module {
                fn update(&mut self, _sample_rate: f32) {
                    let channels = self.channel_count();
                    update::<[<$Module Rounding>]>(
                        &self.params.input,
                        &mut self.outputs.sample,
                        channels,
                    );
                }

                fn process_block(
                    &mut self,
                    outputs: &mut [<$Module BlockOutputs>],
                    range: Range<usize>,
                    _sample_rate: f32,
                ) -> Option<()> {
                    let channels = self.channel_count();
                    process_block::<[<$Module Rounding>]>(
                        &self.params.input,
                        &mut outputs.sample,
                        range,
                        channels,
                    )
                }
            }

            message_handlers!(impl $Module {});
        }
    };
}

const ROUND_DOCS: ModuleDocs = ModuleDocs {
    summary: "Round to whole volts",
    description: "Rounds a signal to the nearest whole volt, halves away from zero: \
                  on a pitch, the nearest octave. A ranged signal keeps its range, \
                  rounded too.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[
        DocExample {
            title: "Semitone steps",
            code: r#"
                $sine(
                  $sine('1hz').range(0, 3).round().div(12).add('c3'),
                ).out()
            "#,
        },
        DocExample {
            title: "Eight steps across an octave",
            code: r#"
                $sine(
                  $sine('1hz').range(0, 7.5).round().range('c3', 'c4'),
                ).out()
            "#,
        },
    ],
};

rounding_module!(Round, name = "$round", docs = ROUND_DOCS, |x| x.round());

const FLOOR_DOCS: ModuleDocs = ModuleDocs {
    summary: "Round down to whole volts",
    description: "Rounds a signal down to a whole volt: on a pitch, the octave at \
                  or below it.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Octaves up from C3",
        code: "$sine($saw('0.5hz').range(0, 4).floor().add('c3')).out()",
    }],
};

rounding_module!(Floor, name = "$floor", docs = FLOOR_DOCS, |x| x.floor());

const CEIL_DOCS: ModuleDocs = ModuleDocs {
    summary: "Round up to whole volts",
    description: "Rounds a signal up to a whole volt: on a pitch, the octave at or \
                  above it.",
    categories: &[ModuleCategory::Math],
    sections: &[],
    examples: &[DocExample {
        title: "Octaves down from C4",
        code: "$sine($saw('0.5hz').range(-4, 0).ceil().add('c4')).out()",
    }],
};

rounding_module!(Ceil, name = "$ceil", docs = CEIL_DOCS, |x| x.ceil());
