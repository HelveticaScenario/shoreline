//! Block-sized port buffer.
//!
//! Layout: a flat `block_size * channels * lanes` slice; sample `index`, channel
//! `ch` starts at `(index * channels + ch) * lanes`. All channel values at the
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

/// A pre-allocated buffer holding `block_size` samples, each with `channels`
/// channels. The channel count is the producing port's width: 1 for a mono
/// (f32) output, the module's channel count for a polyphonic output.
///
/// An f64 port stores each channel as a [`split_f64`] pair (two lanes), so
/// f32 readers load the first lane and never convert, and f64 readers rejoin
/// both.
///
/// Reads cycle (`ch % channels`) so a consumer with more channels than the
/// producer still sees the producer's value broadcast/wrapped across its
/// channels — preserving the mono→poly broadcast semantics.
pub struct BlockPort {
    /// Flat `block_size * channels * lanes` values. Length never changes after
    /// construction.
    data: Box<[f32]>,
    /// Per-port channel width (`>= 1` for live ports, `0` only when the
    /// module is zero-channel).
    channels: usize,
    /// f32 values per channel: 1, or 2 for an f64 port.
    lanes: usize,
    /// Number of sample slots (the internal block size).
    block_size: usize,
}

impl BlockPort {
    /// Allocate a new zeroed f32 port buffer for the given block size and
    /// channel width.
    ///
    /// **Must not be called on the audio thread** (allocates heap memory).
    pub fn new(block_size: usize, channels: usize) -> Self {
        Self::with_lanes(block_size, channels, 1)
    }

    /// Allocate a new zeroed f64 port buffer. Main thread only, like [`new`](Self::new).
    pub fn new_f64(block_size: usize, channels: usize) -> Self {
        Self::with_lanes(block_size, channels, 2)
    }

    fn with_lanes(block_size: usize, channels: usize, lanes: usize) -> Self {
        Self {
            data: vec![0.0f32; block_size * channels * lanes].into_boxed_slice(),
            channels,
            lanes,
            block_size,
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
    pub fn lanes(&self) -> usize {
        self.lanes
    }

    /// Slots `range` of an f32 port, flattened: slot `i`, channel `ch` is
    /// element `(i - range.start) * channels + ch`. Panics if `range` is out
    /// of range.
    #[inline]
    pub fn slots_mut(&mut self, range: std::ops::Range<usize>) -> &mut [f32] {
        debug_assert_eq!(self.lanes, 1, "f64 ports are written with set_f64");
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
        self.data[(index * self.channels + ch % self.channels) * self.lanes]
    }

    /// [`get`](Self::get) on a port built with [`new`](Self::new), skipping
    /// the lane stride. Output structs use it for ports declared f32.
    #[inline]
    pub fn get_f32_port(&self, index: usize, ch: usize) -> f32 {
        debug_assert_eq!(self.lanes, 1, "f64 ports are read with get");
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
        let at = (index * self.channels + ch % self.channels) * self.lanes;
        if self.lanes == 2 {
            join_f64(self.data[at], self.data[at + 1])
        } else {
            self.data[at] as f64
        }
    }

    /// All channels of sample slot `index` of an f32 port. Panics if `index`
    /// is out of range.
    #[inline]
    pub fn slot_mut(&mut self, index: usize) -> &mut [f32] {
        debug_assert_eq!(self.lanes, 1, "f64 ports are written with set_f64");
        let start = index * self.channels;
        &mut self.data[start..start + self.channels]
    }

    /// Write value at `(index, ch)`. Silently ignored if out of range. The
    /// channel index is raw (no cycling): callers write `0..channels`.
    #[inline]
    pub fn set(&mut self, index: usize, ch: usize, value: f32) {
        if index < self.block_size && ch < self.channels {
            let at = (index * self.channels + ch) * self.lanes;
            self.data[at] = value;
            if self.lanes == 2 {
                self.data[at + 1] = 0.0;
            }
        }
    }

    /// [`set`](Self::set) on a port built with [`new`](Self::new), skipping
    /// the lane stride. Output structs use it for ports declared f32.
    #[inline]
    pub fn set_f32_port(&mut self, index: usize, ch: usize, value: f32) {
        debug_assert_eq!(self.lanes, 1, "f64 ports are written with set_f64");
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
            let at = (index * self.channels + ch) * self.lanes;
            let (hi, lo) = split_f64(value);
            self.data[at] = hi;
            if self.lanes == 2 {
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
