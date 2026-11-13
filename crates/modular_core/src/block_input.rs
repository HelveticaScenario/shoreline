//! Whole-block input reads for modules declared with `#[module(block)]`.
//!
//! A block module has a per-sample `update`, which runs inside feedback cycles
//! (Sample mode), and a `process_block(outputs, range, sample_rate)`, which
//! runs everywhere else (Block mode). Both share the module's per-sample math.
//! `process_block` resolves every input for the whole `range` up front into a
//! [`BlockInput`], then runs plain loops over lane rows — no per-read
//! dispatch between constants and cables, no lane cycling, and no copy
//! through an intermediate output struct. [`map`] covers the common
//! elementwise case.
//!
//! An input that cannot be read in place is copied into the caller's
//! [`InputScratch`]: a mono cable broadcast across wider consumer lanes
//! straight from the producer's buffer, and anything else — a cable from a
//! feedback cycle, a narrower poly cable cycled across wider lanes,
//! permuted lanes, a mix of cables and constants — one slot at a time,
//! exactly as a per-sample read would see it. When the range is too long for
//! the scratch, resolution returns `None`, and `process_block` returns `None`
//! too; it must therefore resolve every input before writing anything. The
//! generated wrapper then retries over sub-ranges of at most
//! [`SCRATCH_LEN`]` / lanes` slots, and runs `update` for any sub-range
//! still declined — the route for a module whose input it cannot read by
//! block at all. Each call starts with the consumer's per-block index at
//! `range.start`.

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::Range;

use crate::BlockPort;
use crate::bounds::Bounds;
use crate::dsp::utils::sanitize;
use crate::poly::{PORT_MAX_LANES, PolySignal};
use crate::types::Signal;

/// Values one [`InputScratch`] holds: a whole 64-slot block of up to four
/// lanes, and at least `PORT_MAX_LANES`, so one slot of any module fits.
pub const SCRATCH_LEN: usize = 4 * PORT_MAX_LANES;

/// Stack storage for an input that cannot be read in place: a constant row
/// broadcast across lanes, or values copied out slot by slot.
pub struct InputScratch(MaybeUninit<[f32; SCRATCH_LEN]>);

impl InputScratch {
    /// Uninitialised; a [`BlockInput`] only reads the values it wrote.
    #[inline(always)]
    pub fn new() -> Self {
        Self(MaybeUninit::uninit())
    }

    fn as_mut_ptr(&mut self) -> *mut f32 {
        self.0.as_mut_ptr() as *mut f32
    }
}

impl Default for InputScratch {
    fn default() -> Self {
        Self::new()
    }
}

/// One input resolved for a range of slots: [`row(i)`](Self::row) is slot
/// `i`'s values across the consumer's lanes.
#[derive(Clone, Copy)]
pub struct BlockInput<'a> {
    /// The first slot's row; slot `i`'s starts at `data + (i - first) * stride`.
    /// Stride 0 repeats one row.
    data: *const f32,
    first: usize,
    /// One past the last resolved slot.
    end: usize,
    stride: usize,
    lanes: usize,
    _rows: PhantomData<&'a [f32]>,
}

impl BlockInput<'_> {
    /// Whether every slot reads the same row, so per-row work done on it can
    /// be hoisted out of the slot loop.
    #[inline(always)]
    pub fn is_constant(&self) -> bool {
        self.stride == 0
    }

    /// Slot `i`'s values. Panics unless `i` lies in the resolved range.
    #[inline(always)]
    pub fn row(&self, i: usize) -> &[f32] {
        assert!(
            (self.first..self.end).contains(&i),
            "slot outside the resolved range"
        );
        // SAFETY: resolution guarantees `lanes` initialised values per row
        // for every slot in the resolved range: a rendered producer buffer
        // (which outlives the call, see `PolyCableView`), the input's own
        // constants, or the borrowed scratch.
        unsafe {
            std::slice::from_raw_parts(self.data.add((i - self.first) * self.stride), self.lanes)
        }
    }
}

impl PolySignal {
    /// Resolve this input for slots `range` across `lanes` consumer
    /// lanes, using `scratch` when it cannot be read in place. See the
    /// module docs for when this returns `None`. Panics if `lanes` exceeds
    /// `PORT_MAX_LANES`.
    #[inline(always)]
    pub fn block_input<'a>(
        &'a self,
        range: Range<usize>,
        lanes: usize,
        scratch: &'a mut InputScratch,
    ) -> Option<BlockInput<'a>> {
        assert!(lanes <= PORT_MAX_LANES, "more lanes than a port holds");
        // Inputs read in place resolve inline; copies go out of line.
        if lanes <= self.lanes() {
            let volts = self.volts();
            if !volts.is_empty() {
                return Some(BlockInput {
                    data: volts.as_ptr(),
                    first: range.start,
                    end: range.end,
                    stride: 0,
                    lanes,
                    _rows: PhantomData,
                });
            }
            if let Some((data, stride)) = self
                .cable_view()
                .and_then(|cable| cable.rendered_to(range.end))
            {
                return Some(BlockInput {
                    // SAFETY: `range.start < range.end`, a rendered slot.
                    data: unsafe { data.add(range.start * stride) },
                    first: range.start,
                    end: range.end,
                    stride,
                    lanes,
                    _rows: PhantomData,
                });
            }
        }
        // One slot reads each lane at the consumer's index, which the
        // wrapper leaves at `range.start`.
        if range.len() == 1 && self.volts().is_empty() {
            let row = scratch.as_mut_ptr();
            for lane in 0..lanes {
                // SAFETY: `lanes <= PORT_MAX_LANES <= SCRATCH_LEN`.
                unsafe { row.add(lane).write(self.get_value(lane)) };
            }
            return Some(BlockInput {
                data: row,
                first: range.start,
                end: range.end,
                stride: 0,
                lanes,
                _rows: PhantomData,
            });
        }
        self.copied_block_input(range, lanes, scratch)
    }

    /// [`block_input`](Self::block_input) for inputs copied into `scratch`.
    #[inline(never)]
    fn copied_block_input<'a>(
        &'a self,
        range: Range<usize>,
        lanes: usize,
        scratch: &'a mut InputScratch,
    ) -> Option<BlockInput<'a>> {
        let width = self.lanes();
        let volts = self.volts();
        if !volts.is_empty() {
            let row = scratch.as_mut_ptr();
            for lane in 0..lanes {
                // SAFETY: `lanes <= PORT_MAX_LANES <= SCRATCH_LEN`.
                unsafe { row.add(lane).write(volts[lane % width]) };
            }
            return Some(BlockInput {
                data: row,
                first: range.start,
                end: range.end,
                stride: 0,
                lanes,
                _rows: PhantomData,
            });
        }
        if range.len() * lanes > SCRATCH_LEN {
            return None;
        }
        if width == 1
            && let Some((data, stride)) = self
                .cable_view()
                .and_then(|cable| cable.rendered_to(range.end))
        {
            let rows = scratch.as_mut_ptr();
            for (n, i) in range.clone().enumerate() {
                // SAFETY: slot `i` is rendered; `range.len() * lanes <=
                // SCRATCH_LEN`.
                unsafe {
                    let value = *data.add(i * stride);
                    for lane in 0..lanes {
                        rows.add(n * lanes + lane).write(value);
                    }
                }
            }
            return Some(BlockInput {
                data: rows,
                first: range.start,
                end: range.end,
                stride: lanes,
                lanes,
                _rows: PhantomData,
            });
        }
        // Each cable reads the slot its consumer's index names, so step that
        // index through the range. The wrapper resets it once `process_block`
        // returns.
        let index = self.signals().iter().find_map(|signal| match signal {
            Signal::Cable { index_ptr, .. } if !index_ptr.is_null() => Some(*index_ptr),
            _ => None,
        });
        let rows = scratch.as_mut_ptr();
        for (n, i) in range.clone().enumerate() {
            if let Some(index) = index {
                // SAFETY: the consumer wrapper's index cell, which outlives
                // its params; audio thread only.
                unsafe { (*index).set(i) };
            }
            for lane in 0..lanes {
                // SAFETY: `range.len() * lanes <= SCRATCH_LEN`.
                unsafe { rows.add(n * lanes + lane).write(self.get_value(lane)) };
            }
        }
        Some(BlockInput {
            data: rows,
            first: range.start,
            end: range.end,
            stride: lanes,
            lanes,
            _rows: PhantomData,
        })
    }
}

/// A disconnected input's row when it defaults to zero, read in place.
static ZERO_ROW: [f32; PORT_MAX_LANES] = [0.0; PORT_MAX_LANES];

/// [`PolySignal::block_input`] for an optional input, broadcasting `default`
/// when it is disconnected.
pub trait OptionBlockInput {
    fn block_input_or<'a>(
        &'a self,
        range: Range<usize>,
        lanes: usize,
        default: f32,
        scratch: &'a mut InputScratch,
    ) -> Option<BlockInput<'a>>;
}

impl OptionBlockInput for Option<PolySignal> {
    #[inline]
    fn block_input_or<'a>(
        &'a self,
        range: Range<usize>,
        lanes: usize,
        default: f32,
        scratch: &'a mut InputScratch,
    ) -> Option<BlockInput<'a>> {
        assert!(lanes <= PORT_MAX_LANES, "more lanes than a port holds");
        match self {
            Some(signal) => signal.block_input(range, lanes, scratch),
            None if default == 0.0 => Some(BlockInput {
                data: ZERO_ROW.as_ptr(),
                first: range.start,
                end: range.end,
                stride: 0,
                lanes,
                _rows: PhantomData,
            }),
            None => {
                let row = scratch.as_mut_ptr();
                for lane in 0..lanes {
                    // SAFETY: `lanes <= PORT_MAX_LANES <= SCRATCH_LEN`.
                    unsafe { row.add(lane).write(default) };
                }
                Some(BlockInput {
                    data: row,
                    first: range.start,
                    end: range.end,
                    stride: 0,
                    lanes,
                    _rows: PhantomData,
                })
            }
        }
    }
}

/// Elements per pass of [`map`]'s flat loop. Also bounds the lane count a
/// repeated row can be expanded across, so it must be at least
/// `PORT_MAX_LANES`.
const CHUNK: usize = PORT_MAX_LANES;

/// Write `f(inputs)` to every lane of `out` for slots `range`, sanitising
/// each result. Panics unless every input was resolved for `range` at `out`'s
/// lane count.
///
/// When every input is either contiguous rows or one repeated row, a
/// multi-slot range is processed as a single flat run, which vectorises at any
/// lane count; otherwise row by row.
#[inline(always)]
pub fn map<const N: usize>(
    out: &mut BlockPort,
    range: Range<usize>,
    inputs: [BlockInput<'_>; N],
    f: impl Fn([f32; N]) -> f32,
) {
    let lanes = out.lanes();
    assert!(
        inputs.iter().all(|input| input.lanes == lanes
            && input.first == range.start
            && input.end == range.end),
        "input resolved for another range or width"
    );
    if lanes == 0 {
        return;
    }
    let flat = range.len() > 1
        && inputs
            .iter()
            .all(|input| input.stride == lanes || input.stride == 0);
    if !flat {
        for i in range {
            let rows: [&[f32]; N] = std::array::from_fn(|j| inputs[j].row(i));
            for (lane, value) in out.slot_mut(i).iter_mut().enumerate() {
                *value = sanitize(f(rows.map(|row| row[lane])));
            }
        }
        return;
    }
    // A repeated row is expanded into a whole number of rows filling (nearly)
    // a chunk, so every chunk starts on a row boundary and reads it from 0.
    let chunk = (CHUNK / lanes * lanes).min(range.len() * lanes);
    let mut repeated = [const { MaybeUninit::<[f32; CHUNK]>::uninit() }; N];
    for (buffer, input) in repeated.iter_mut().zip(&inputs) {
        if input.stride == 0 {
            // SAFETY: `chunk <= CHUNK`; only these elements are written and,
            // below, read.
            let buffer =
                unsafe { std::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut f32, chunk) };
            let row = input.row(input.first);
            if let [value] = row {
                buffer.fill(*value);
            } else {
                for rows in buffer.chunks_exact_mut(lanes) {
                    for (value, &v) in rows.iter_mut().zip(row) {
                        *value = v;
                    }
                }
            }
        }
    }
    for (n, out_chunk) in out.slots_mut(range).chunks_mut(chunk).enumerate() {
        let len = out_chunk.len();
        let offset = n * chunk;
        let sources: [&[f32]; N] = std::array::from_fn(|j| {
            let input = &inputs[j];
            if input.stride == 0 {
                // SAFETY: the first `chunk >= len` elements were written above.
                unsafe { std::slice::from_raw_parts(repeated[j].as_ptr() as *const f32, len) }
            } else {
                // SAFETY: contiguous rows from the range's first slot;
                // `offset + len` stays within the range's rows.
                unsafe { std::slice::from_raw_parts(input.data.add(offset), len) }
            }
        });
        for (k, value) in out_chunk.iter_mut().enumerate() {
            // SAFETY: every source has length `len` and `k < len`.
            *value = sanitize(f(sources.map(|source| unsafe { *source.get_unchecked(k) })));
        }
    }
}

/// Write `out`'s bounds for every slot of `range` from each slot's `values`:
/// `rule(lane, values)` per lane. When every value holds still one row fills
/// the range; otherwise each slot is computed. A rule gives `None` only for
/// bounds that move within the range, so a `None` on the first slot writes
/// nothing and returns `None`, letting the block path decline. Call it before
/// writing the samples.
pub fn map_bounds<const N: usize>(
    out: &mut BlockPort,
    range: Range<usize>,
    values: [BlockInput<'_>; N],
    rule: impl Fn(usize, [f32; N]) -> Option<Bounds>,
) -> Option<()> {
    if !out.has_bounds() || range.is_empty() {
        return Some(());
    }
    let first: [&[f32]; N] = std::array::from_fn(|j| values[j].row(range.start));
    if values.iter().all(|value| value.is_constant()) {
        return out.fill_bounds_with(range, |lane| rule(lane, first.map(|row| row[lane])));
    }
    let lanes = out.lanes();
    if (0..lanes).any(|lane| rule(lane, first.map(|row| row[lane])).is_none()) {
        return None;
    }
    let start = range.start;
    let bounds = out.bounds_slots_mut(range.clone());
    for i in range {
        let rows: [&[f32]; N] = std::array::from_fn(|j| values[j].row(i));
        let slot = &mut bounds[(i - start) * lanes..(i - start + 1) * lanes];
        for (lane, b) in slot.iter_mut().enumerate() {
            *b = rule(lane, rows.map(|row| row[lane])).unwrap_or(Bounds::UNBOUNDED);
        }
    }
    Some(())
}

/// A [`BlockInput`] that holds `value(lane)` on each of `lanes` lanes for
/// every slot of `range`, built in `scratch`: a value a module derives once per
/// range, read like any other input.
pub fn constant_row<'a>(
    range: Range<usize>,
    lanes: usize,
    scratch: &'a mut InputScratch,
    value: impl Fn(usize) -> f32,
) -> BlockInput<'a> {
    assert!(lanes <= PORT_MAX_LANES, "more lanes than a port holds");
    let row = scratch.as_mut_ptr();
    for lane in 0..lanes {
        // SAFETY: `lanes <= PORT_MAX_LANES <= SCRATCH_LEN`.
        unsafe { row.add(lane).write(value(lane)) };
    }
    BlockInput {
        data: row,
        first: range.start,
        end: range.end,
        stride: 0,
        lanes,
        _rows: PhantomData,
    }
}
