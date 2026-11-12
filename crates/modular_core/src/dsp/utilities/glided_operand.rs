//! The shared update and block paths of `$amp`, `$shift` and `$gain`: an
//! input combined channel by channel with an operand that glides across
//! patch updates.

use std::ops::Range;

use crate::block_input::{self, InputScratch};
use crate::block_port::BlockPort;
use crate::dsp::utils::Sample;
use crate::poly::{PolyOutput, PolySignal};
use crate::types::SwapGlide;

/// How a module combines its input with its operand.
pub(super) trait Combine {
    fn apply<T: Sample>(input: T, operand: T) -> T;
}

/// One sample of every channel. Outside a glide the operand is read raw,
/// exactly as [`process_block`] reads it, and at full precision when `out`
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
            let value = if glide.will_glide(mark) {
                glide.follow(operand.get_value(i), mark, sample_rate) as f64
            } else {
                let value = operand.get_value_f64(i);
                glide.settle(value as f32, mark);
                value
            };
            out.set_f64(i, C::apply(input.get_value_f64(i), value));
        }
        return;
    }
    for (i, glide) in glides[..channels].iter_mut().enumerate() {
        let value = glide.follow(operand.get_value(i), operand.glide_mark(i), sample_rate);
        out.set(i, C::apply(input.get_value(i), value));
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
    operand.settle_glides(glides, y.row(range.end - 1));
    block_input::map(out, range, [x, y], |[x, y]| C::apply(x, y));
    Some(())
}
