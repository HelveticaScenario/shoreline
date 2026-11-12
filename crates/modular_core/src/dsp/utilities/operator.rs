//! Operator modules: an input combined channel by channel with one operand
//! that glides when it is edited, so a changed amount never clicks. Each is
//! stamped by [`operator_module!`] from the combining expression; the update
//! and block paths here are shared by all of them.

use std::ops::Range;

use crate::block_input::{self, InputScratch};
use crate::block_port::BlockPort;
use crate::dsp::utils::Sample;
use crate::poly::{PolyOutput, PolySignal};
use crate::types::SwapGlide;

/// How a module combines its input with its operand.
pub(super) trait Combine {
    fn apply<T: Sample>(input: T, operand: T) -> T;

    /// The form of the operand that glides. A module whose result moves too
    /// far for small steps of its operand glides a steadier form, and
    /// combines with it through [`apply_glided`](Self::apply_glided) only
    /// while a glide runs; settled, it applies the operand exactly.
    #[inline(always)]
    fn glide_form<T: Sample>(operand: T) -> T {
        operand
    }

    /// [`apply`](Self::apply) with the operand in its [`glide_form`](Self::glide_form).
    #[inline(always)]
    fn apply_glided<T: Sample>(input: T, form: T) -> T {
        Self::apply(input, form)
    }
}

/// One sample of every channel. Outside a glide the operand is applied raw,
/// exactly as [`process_block`] applies it, and at full precision when `out`
/// is f64, since the operand may be an f64 cable itself.
#[inline(always)]
pub(super) fn update<C: Combine>(
    input: &PolySignal,
    operand: &PolySignal,
    glides: &mut [SwapGlide],
    out: &mut PolyOutput,
    channels: usize,
    sample_rate: f32,
) {
    if out.is_f64() {
        for (i, glide) in glides[..channels].iter_mut().enumerate() {
            let mark = operand.glide_mark(i);
            let x = input.get_value_f64(i);
            let value = if glide.will_glide(mark) {
                let form = glide.follow(C::glide_form(operand.get_value(i)), mark, sample_rate);
                if glide.gliding() {
                    C::apply_glided(x, form as f64)
                } else {
                    C::apply(x, operand.get_value_f64(i))
                }
            } else {
                let y = operand.get_value_f64(i);
                glide.settle(C::glide_form(y) as f32, mark);
                C::apply(x, y)
            };
            out.set_f64(i, value);
        }
        return;
    }
    for (i, glide) in glides[..channels].iter_mut().enumerate() {
        let (x, y) = (input.get_value(i), operand.get_value(i));
        let form = glide.follow(C::glide_form(y), operand.glide_mark(i), sample_rate);
        let value = if glide.gliding() {
            C::apply_glided(x, form)
        } else {
            C::apply(x, y)
        };
        out.set(i, value);
    }
}

/// The block path; `None` while any channel's operand glides.
#[inline(always)]
pub(super) fn process_block<C: Combine>(
    input: &PolySignal,
    operand: &PolySignal,
    glides: &mut [SwapGlide],
    out: &mut BlockPort,
    range: Range<usize>,
    channels: usize,
) -> Option<()> {
    let glides = &mut glides[..channels];
    if operand.glides_pending(glides) {
        return None;
    }
    let (mut s0, mut s1) = (InputScratch::new(), InputScratch::new());
    let x = input.block_input(range.clone(), channels, &mut s0)?;
    let y = operand.block_input(range.clone(), channels, &mut s1)?;
    for (i, (glide, &last)) in glides.iter_mut().zip(y.row(range.end - 1)).enumerate() {
        glide.settle(C::glide_form(last), operand.glide_mark(i));
    }
    block_input::map(out, range, [x, y], |[x, y]| C::apply(x, y));
    Some(())
}

/// Stamp an operator module: `input` and one operand (the two positional
/// args), one output at the precision of its inputs, and `|x, y| combine`
/// applied per sample at either precision. An optional
/// `glide = (|y| form, |x, form| combine)` sets [`Combine::glide_form`] and
/// [`Combine::apply_glided`]. The expressions run over a generic [`Sample`]
/// `T`.
macro_rules! operator_module {
    (
        $Module:ident,
        name = $name:literal,
        docs = $docs:path,
        input = $input_doc:literal,
        operand = $operand:ident: $operand_doc:literal
            $(, default = $default:literal)?
            $(, range = $range:tt)?
            , control_range = ($lo:literal, $hi:literal),
        output = $output_doc:literal,
        $(glide = (|$g:ident| $form:expr, |$gx:ident, $gf:ident| $glided:expr),)?
        |$x:ident, $y:ident| $combine:expr $(,)?
    ) => {
        paste::paste! {
            #[params]
            #[derive(Clone, deserr::Deserr, schemars::JsonSchema, Connect, ChannelCount)]
            #[serde(rename_all = "camelCase")]
            #[deserr(rename_all = camelCase, deny_unknown_fields)]
            struct [<$Module Params>] {
                #[input($input_doc, type = audio)]
                input: crate::poly::PolySignal,
                #[input(
                    $operand_doc,
                    type = control,
                    $(default = $default,)?
                    $(range = $range,)?
                    control_range = ($lo, $hi),
                )]
                $operand: crate::poly::PolySignal,
            }

            #[derive(Outputs, schemars::JsonSchema)]
            #[serde(rename_all = "camelCase")]
            struct [<$Module Outputs>] {
                #[output("output", $output_doc, type = cv, precision = inherit, default)]
                sample: crate::poly::PolyOutput,
            }

            #[module(name = $name, docs = $docs, args(input, $operand), block)]
            pub struct $Module {
                outputs: [<$Module Outputs>],
                params: [<$Module Params>],
                channel_state: Box<[crate::types::SwapGlide]>,
            }

            struct [<$Module Combine>];

            impl crate::dsp::utilities::operator::Combine for [<$Module Combine>] {
                #[inline(always)]
                fn apply<T: crate::dsp::utils::Sample>($x: T, $y: T) -> T {
                    $combine
                }

                $(
                    #[inline(always)]
                    fn glide_form<T: crate::dsp::utils::Sample>($g: T) -> T {
                        $form
                    }

                    #[inline(always)]
                    fn apply_glided<T: crate::dsp::utils::Sample>($gx: T, $gf: T) -> T {
                        $glided
                    }
                )?
            }

            impl $Module {
                fn update(&mut self, sample_rate: f32) {
                    let channels = self.channel_count();
                    crate::dsp::utilities::operator::update::<[<$Module Combine>]>(
                        &self.params.input,
                        &self.params.$operand,
                        &mut self.channel_state,
                        &mut self.outputs.sample,
                        channels,
                        sample_rate,
                    );
                }

                fn process_block(
                    &mut self,
                    outputs: &mut [<$Module BlockOutputs>],
                    range: std::ops::Range<usize>,
                    _sample_rate: f32,
                ) -> Option<()> {
                    let channels = self.channel_count();
                    crate::dsp::utilities::operator::process_block::<[<$Module Combine>]>(
                        &self.params.input,
                        &self.params.$operand,
                        &mut self.channel_state,
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

pub(super) use operator_module;
