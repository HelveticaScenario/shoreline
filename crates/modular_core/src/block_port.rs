//! Block-sized port buffer.
//!
//! Layout: a flat `block_size * channels * words` slice; sample `index`, channel
//! `ch` starts at `(index * channels + ch) * words`. All channel values at the
//! same sample index are contiguous in memory. Heap-allocated once at
//! construction; never resized on the audio thread.

/// An f64 as two f32s: its nearest f32, then the f32 nearest the remainder.
/// The pair carries about 48 significant bits, and the first half alone is
/// exactly what an f32 reader would get from a cast.
#[inline]
pub fn split_f64(value: f64) -> (f32, f32) {
    let hi = value as f32;
    (hi, (value - hi as f64) as f32)
}

/// Rejoin a [`split_f64`] pair.
#[inline]
pub fn join_f64(hi: f32, lo: f32) -> f64 {
    hi as f64 + lo as f64
}

use crate::bounds::Bounds;
use crate::poly::PORT_MAX_CHANNELS;

/// A pre-allocated buffer holding `block_size` samples, each with `channels`
/// channels. The channel count is the producing port's width: 1 for a mono
/// (f32) output, the module's channel count for a polyphonic output.
///
/// An f64 port stores each channel as a [`split_f64`] pair (two words), so
/// f32 readers load the first word and never convert, and f64 readers rejoin
/// both.
///
/// Reads cycle (`ch % channels`) so a consumer with more channels than the
/// producer still sees the producer's value broadcast/wrapped across its
/// channels — preserving the mono→poly broadcast semantics.
///
/// A bounded port also stores one [`Bounds`] per channel per slot, laid out
/// like f32 samples and written with them.
pub struct BlockPort {
    /// Flat `block_size * channels * words` values. Length never changes after
    /// construction.
    data: Box<[f32]>,
    /// Per-port channel width (`>= 1` for live ports, `0` only when the
    /// module is zero-channel).
    channels: usize,
    /// f32 values per channel: 1, or 2 for an f64 port.
    words: usize,
    /// Number of sample slots (the internal block size).
    block_size: usize,
    /// The bounds of a bounded port.
    bounds: Option<Box<BoundsStore>>,
}

/// A bounded port's bounds: a row per slot, `channels` to a row. Bounds
/// rarely move within a block, so the slots from 0 while they hold one row
/// are kept as that row and a count, and only written out once a different
/// row arrives.
pub struct BoundsStore {
    slots: Box<[Bounds]>,
    /// Slots `0..run` hold `row` and are not written to `slots`.
    run: usize,
    row: Box<[Bounds]>,
    /// One past the last slot written this block.
    #[cfg(debug_assertions)]
    written: usize,
}

/// `ch % channels`, skipping the division in the usual case of a channel the
/// port has.
#[inline(always)]
fn cycle(ch: usize, channels: usize) -> usize {
    if ch < channels { ch } else { ch % channels }
}

impl BoundsStore {
    #[inline]
    fn channels(&self) -> usize {
        self.row.len()
    }

    /// The bounds at slot `index`, channel `ch` (cycling).
    #[inline]
    pub fn at(&self, index: usize, ch: usize) -> Bounds {
        let ch = cycle(ch, self.channels());
        if index < self.run {
            self.row[ch]
        } else {
            self.slots[index * self.channels() + ch]
        }
    }

    /// Channel `ch`'s bounds when they hold one value over every slot of
    /// `range`.
    #[inline]
    pub fn uniform(&self, range: std::ops::Range<usize>, ch: usize) -> Option<Bounds> {
        if range.end <= self.run {
            return Some(self.row[cycle(ch, self.channels())]);
        }
        let first = self.at(range.start, ch);
        range
            .skip(1)
            .all(|slot| self.at(slot, ch) == first)
            .then_some(first)
    }

    /// Write `row` to every slot of `range`, extending the run when it
    /// continues it.
    #[inline]
    fn write(&mut self, range: std::ops::Range<usize>, row: &[Bounds]) {
        #[cfg(debug_assertions)]
        {
            self.written = self.written.max(range.end);
        }
        if range.start == self.run && (self.run == 0 || *self.row == *row) {
            if self.run == 0 {
                self.row.copy_from_slice(row);
            }
            self.run = range.end;
            return;
        }
        self.settle();
        let channels = self.channels();
        for slot in
            self.slots[range.start * channels..range.end * channels].chunks_exact_mut(channels)
        {
            slot.copy_from_slice(row);
        }
    }

    /// Write the run out to its slots.
    #[inline]
    fn settle(&mut self) {
        let channels = self.channels();
        for slot in self.slots[..self.run * channels].chunks_exact_mut(channels) {
            slot.copy_from_slice(&self.row);
        }
        self.run = 0;
    }

    #[inline]
    fn start_block(&mut self) {
        let channels = self.channels();
        let block_size = self.slots.len() / channels;
        if self.run == block_size {
            let last = (block_size - 1) * channels;
            self.slots[last..last + channels].copy_from_slice(&self.row);
        }
        self.run = 0;
        #[cfg(debug_assertions)]
        {
            self.written = 0;
        }
    }
}

impl BlockPort {
    /// Allocate a new zeroed f32 port buffer for the given block size and
    /// channel width.
    ///
    /// **Must not be called on the audio thread** (allocates heap memory).
    pub fn new(block_size: usize, channels: usize) -> Self {
        Self::with_words(block_size, channels, 1)
    }

    /// Allocate a new zeroed f64 port buffer. Main thread only, like [`new`](Self::new).
    pub fn new_f64(block_size: usize, channels: usize) -> Self {
        Self::with_words(block_size, channels, 2)
    }

    fn with_words(block_size: usize, channels: usize, words: usize) -> Self {
        Self {
            data: vec![0.0f32; block_size * channels * words].into_boxed_slice(),
            channels,
            words,
            block_size,
            bounds: None,
        }
    }

    /// Give this port bounds storage, every slot holding 0 V's bounds until
    /// written, as the samples hold 0 V. Main thread only, like
    /// [`new`](Self::new); a zero-channel port stores none.
    pub fn make_bounded(&mut self) {
        if self.channels == 0 {
            return;
        }
        let zero = Bounds::point(0.0);
        self.bounds = Some(Box::new(BoundsStore {
            slots: vec![zero; self.block_size * self.channels].into_boxed_slice(),
            run: 0,
            row: vec![zero; self.channels].into_boxed_slice(),
            #[cfg(debug_assertions)]
            written: 0,
        }));
    }

    /// Whether every slot below `end` has had its bounds written this block;
    /// checked in debug builds after a module renders, and always true
    /// otherwise.
    #[inline]
    pub fn bounds_written(&self, end: usize) -> bool {
        #[cfg(debug_assertions)]
        if let Some(store) = &self.bounds {
            return store.written >= end;
        }
        let _ = end;
        true
    }

    /// Whether this port stores bounds.
    #[inline]
    pub fn has_bounds(&self) -> bool {
        self.bounds.is_some()
    }

    /// The bounds storage, for cables that read it directly; `None` on a port
    /// without bounds. It lives on the heap, so the pointer outlives moves of
    /// the port.
    #[inline]
    pub fn bounds_store(&self) -> Option<*const BoundsStore> {
        self.bounds
            .as_deref()
            .map(|store| store as *const BoundsStore)
    }

    /// The bounds at `(index, ch)`, cycling channels like [`get`](Self::get);
    /// [`Bounds::UNBOUNDED`] on a port without bounds or out of range.
    #[inline]
    pub fn bounds_at(&self, index: usize, ch: usize) -> Bounds {
        match &self.bounds {
            Some(store) if index < self.block_size && self.channels > 0 => store.at(index, ch),
            _ => Bounds::UNBOUNDED,
        }
    }

    /// Write slot `index`'s bounds from `row`, cycled across this port's
    /// channels; ignored on a port without bounds, out of range or for an
    /// empty row.
    #[inline]
    pub fn set_bounds_row(&mut self, index: usize, row: &[Bounds]) {
        let channels = self.channels.min(PORT_MAX_CHANNELS);
        let Some(store) = &mut self.bounds else {
            return;
        };
        if index >= self.block_size || row.is_empty() {
            return;
        }
        if row.len() == channels {
            store.write(index..index + 1, row);
            return;
        }
        let mut cycled = [const { std::mem::MaybeUninit::uninit() }; PORT_MAX_CHANNELS];
        for (ch, bounds) in cycled[..channels].iter_mut().enumerate() {
            bounds.write(row[ch % row.len()]);
        }
        // SAFETY: the first `channels` entries were just written.
        let cycled =
            unsafe { std::slice::from_raw_parts(cycled.as_ptr() as *const Bounds, channels) };
        store.write(index..index + 1, cycled);
    }

    /// Write one row to every slot of `range`: `rule(ch)` for each of this
    /// port's channels. When any channel's rule gives `None`, writes nothing
    /// and returns `None`, so a block path can decline. A port without bounds
    /// skips the rule.
    #[inline]
    pub fn fill_bounds_with(
        &mut self,
        range: std::ops::Range<usize>,
        rule: impl Fn(usize) -> Option<Bounds>,
    ) -> Option<()> {
        let channels = self.channels.min(PORT_MAX_CHANNELS);
        let Some(store) = &mut self.bounds else {
            return Some(());
        };
        let mut row = [const { std::mem::MaybeUninit::uninit() }; PORT_MAX_CHANNELS];
        for (ch, bounds) in row[..channels].iter_mut().enumerate() {
            bounds.write(rule(ch)?);
        }
        // SAFETY: the first `channels` entries were just written.
        let row = unsafe { std::slice::from_raw_parts(row.as_ptr() as *const Bounds, channels) };
        store.write(range, row);
        Some(())
    }

    /// The bounds of slots `range`, flattened like
    /// [`slots_mut`](Self::slots_mut), for a writer whose bounds change from
    /// slot to slot; empty on a port without bounds.
    #[inline]
    pub fn bounds_slots_mut(&mut self, range: std::ops::Range<usize>) -> &mut [Bounds] {
        let channels = self.channels;
        match &mut self.bounds {
            Some(store) => {
                store.settle();
                #[cfg(debug_assertions)]
                {
                    store.written = store.written.max(range.end);
                }
                &mut store.slots[range.start * channels..range.end * channels]
            }
            None => &mut [],
        }
    }

    /// Begin a block: the stored bounds start afresh, the previous block's
    /// last slot kept for a feedback cycle's delayed read of it.
    #[inline]
    pub fn start_bounds_block(&mut self) {
        if let Some(store) = &mut self.bounds {
            store.start_block();
        }
    }

    /// Start of the flat sample data, for cables that read it directly.
    #[inline]
    pub fn as_ptr(&self) -> *const f32 {
        self.data.as_ptr()
    }

    /// This port's channel width.
    #[inline]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// f32 values per channel: 2 for an f64 port, otherwise 1.
    #[inline]
    pub fn words(&self) -> usize {
        self.words
    }

    /// Slots `range` of an f32 port, flattened: slot `i`, channel `ch` is
    /// element `(i - range.start) * channels + ch`. Panics if `range` is out
    /// of range.
    #[inline]
    pub fn slots_mut(&mut self, range: std::ops::Range<usize>) -> &mut [f32] {
        debug_assert_eq!(self.words, 1, "f64 ports are written with set_f64");
        &mut self.data[range.start * self.channels..range.end * self.channels]
    }

    /// Read value at `(index, ch)`, returning `0.0` for out-of-range sample
    /// indices or a zero-channel port. The channel index cycles modulo the
    /// port width, so a consumer reading a higher channel than the producer
    /// has wraps around — preserving mono-broadcast / poly-cycling semantics.
    /// An f64 port reads as its nearest f32.
    #[inline]
    pub fn get(&self, index: usize, ch: usize) -> f32 {
        if self.channels == 0 || index >= self.block_size {
            return 0.0;
        }
        self.data[(index * self.channels + ch % self.channels) * self.words]
    }

    /// [`get`](Self::get) on a port built with [`new`](Self::new), skipping
    /// the word stride. Output structs use it for ports declared f32.
    #[inline]
    pub fn get_f32_port(&self, index: usize, ch: usize) -> f32 {
        debug_assert_eq!(self.words, 1, "f64 ports are read with get");
        if self.channels == 0 || index >= self.block_size {
            return 0.0;
        }
        self.data[index * self.channels + ch % self.channels]
    }

    /// Like [`get`](Self::get), at full precision on an f64 port.
    #[inline]
    pub fn get_f64(&self, index: usize, ch: usize) -> f64 {
        if self.channels == 0 || index >= self.block_size {
            return 0.0;
        }
        let at = (index * self.channels + ch % self.channels) * self.words;
        if self.words == 2 {
            join_f64(self.data[at], self.data[at + 1])
        } else {
            self.data[at] as f64
        }
    }

    /// All channels of sample slot `index` of an f32 port. Panics if `index`
    /// is out of range.
    #[inline]
    pub fn slot_mut(&mut self, index: usize) -> &mut [f32] {
        debug_assert_eq!(self.words, 1, "f64 ports are written with set_f64");
        let start = index * self.channels;
        &mut self.data[start..start + self.channels]
    }

    /// Write value at `(index, ch)`. Silently ignored if out of range. The
    /// channel index is raw (no cycling): callers write `0..channels`.
    #[inline]
    pub fn set(&mut self, index: usize, ch: usize, value: f32) {
        if index < self.block_size && ch < self.channels {
            let at = (index * self.channels + ch) * self.words;
            self.data[at] = value;
            if self.words == 2 {
                self.data[at + 1] = 0.0;
            }
        }
    }

    /// [`set`](Self::set) on a port built with [`new`](Self::new), skipping
    /// the word stride. Output structs use it for ports declared f32.
    #[inline]
    pub fn set_f32_port(&mut self, index: usize, ch: usize, value: f32) {
        debug_assert_eq!(self.words, 1, "f64 ports are written with set_f64");
        if index < self.block_size && ch < self.channels {
            self.data[index * self.channels + ch] = value;
        }
    }

    /// Write value at `(index, ch)` at full precision on an f64 port; an f32
    /// port stores its nearest f32. Out-of-range writes are ignored, as in
    /// [`set`](Self::set).
    #[inline]
    pub fn set_f64(&mut self, index: usize, ch: usize, value: f64) {
        if index < self.block_size && ch < self.channels {
            let at = (index * self.channels + ch) * self.words;
            let (hi, lo) = split_f64(value);
            self.data[at] = hi;
            if self.words == 2 {
                self.data[at + 1] = lo;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_port_new_zeroed() {
        let bp = BlockPort::new(4, 16);
        assert_eq!(bp.channels(), 16);
        for index in 0..4 {
            for ch in 0..16 {
                assert_eq!(bp.get(index, ch), 0.0);
            }
        }
    }

    #[test]
    fn block_port_get_in_range() {
        let mut bp = BlockPort::new(4, 16);
        bp.set(2, 3, 1.5);
        assert_eq!(bp.get(2, 3), 1.5);
    }

    #[test]
    fn block_port_get_out_of_range() {
        let bp = BlockPort::new(4, 16);
        // Sample index out of range → 0.0.
        assert_eq!(bp.get(99, 0), 0.0);
    }

    #[test]
    fn block_port_channel_cycles() {
        // A mono (width-1) port broadcasts to every consumer channel.
        let mut bp = BlockPort::new(4, 1);
        bp.set(1, 0, 2.5);
        assert_eq!(bp.get(1, 0), 2.5);
        assert_eq!(bp.get(1, 5), 2.5); // 5 % 1 == 0
        // A width-2 port wraps higher channels.
        let mut poly = BlockPort::new(4, 2);
        poly.set(0, 0, 1.0);
        poly.set(0, 1, 2.0);
        assert_eq!(poly.get(0, 2), 1.0); // 2 % 2 == 0
        assert_eq!(poly.get(0, 3), 2.0); // 3 % 2 == 1
    }

    #[test]
    fn block_port_f64_round_trips() {
        let mut bp = BlockPort::new_f64(4, 2);
        let bars = 1_000_003.000_012_5;
        bp.set_f64(1, 1, bars);
        assert_eq!(bp.get_f64(1, 1), bars);
        assert_eq!(bp.get(1, 1), bars as f32);
        assert_eq!(bp.get_f64(1, 3), bars); // cycles like f32 reads
        bp.set(2, 0, 1.5);
        assert_eq!(bp.get_f64(2, 0), 1.5);
    }

    #[test]
    fn block_port_set() {
        let mut bp = BlockPort::new(4, 16);
        bp.set(1, 2, 2.5);
        assert!((bp.get(1, 2) - 2.5).abs() < 1e-6);
    }
}
