//! `plaits::Ensemble`: a stereo chorus of three modulated taps per side,
//! crossed between the sides, swept by a slow and a fast LFO at three phases.

use crate::dsp::mi::core::sine::sine_raw;

/// Plaits' per-side delay, in samples at 48 kHz.
const LINE: f32 = 512.0;
/// The taps' centre, and their slow and fast sweeps per unit of depth, in
/// samples at 48 kHz.
const CENTRE: f32 = 192.0;
const SLOW_SWEEP: f32 = 160.0;
const FAST_SWEEP: f32 = 16.0;
/// Plaits' LFO increments per 48 kHz sample: 0.75 Hz and 6.57 Hz.
const SLOW_INCREMENT: f64 = 67289.0;
const FAST_INCREMENT: f64 = 589980.0;
const ONE_THIRD: u32 = 1417339207;
const TWO_THIRDS: u32 = 2834678415;

/// Per-rate constants.
#[derive(Clone, Copy, Default)]
pub struct EnsembleRates {
    size: usize,
    to_48k: f32,
    slow_increment: u32,
    fast_increment: u32,
}

impl EnsembleRates {
    pub fn new(sample_rate: f32) -> Self {
        let to_48k = sample_rate / 48000.0;
        let per_sample =
            |increment: f64| (increment * 48000.0 / f64::from(sample_rate)).round() as u32;
        Self {
            size: ((LINE * to_48k).ceil() as usize).next_power_of_two(),
            to_48k,
            slow_increment: per_sample(SLOW_INCREMENT),
            fast_increment: per_sample(FAST_INCREMENT),
        }
    }
}

/// A delay line written each sample and read at fractional offsets behind
/// the write.
#[derive(Default)]
struct Line {
    buffer: Box<[f32]>,
    mask: usize,
}

impl Line {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size].into_boxed_slice(),
            mask: size - 1,
        }
    }

    #[inline(always)]
    fn read(&self, write_ptr: usize, offset: f32) -> f32 {
        let integral = offset as usize;
        let fractional = offset - integral as f32;
        let a = self.buffer[(write_ptr + integral) & self.mask];
        let b = self.buffer[(write_ptr + integral + 1) & self.mask];
        a + (b - a) * fractional
    }
}

/// Empty until `init` replaces it with [`Ensemble::new`]; never processed
/// before then.
#[derive(Default)]
pub struct Ensemble {
    left: Line,
    right: Line,
    write_ptr: usize,
    phase_1: u32,
    phase_2: u32,
}

impl Ensemble {
    /// A silent ensemble sized for `rates`. Allocates.
    pub fn new(rates: &EnsembleRates) -> Self {
        Self {
            left: Line::new(rates.size),
            right: Line::new(rates.size),
            write_ptr: 0,
            phase_1: 0,
            phase_2: 0,
        }
    }

    /// One stereo sample: `amount` (0–1) of the chorus mixed in, its taps
    /// swept by `depth` (0–1).
    #[inline(always)]
    pub fn process(
        &mut self,
        left: f32,
        right: f32,
        amount: f32,
        depth: f32,
        rates: &EnsembleRates,
    ) -> (f32, f32) {
        self.write_ptr = if self.write_ptr == 0 {
            self.left.mask
        } else {
            self.write_ptr - 1
        };
        let dry_amount = 1.0 - amount * 0.5;

        self.phase_1 = self.phase_1.wrapping_add(rates.slow_increment);
        self.phase_2 = self.phase_2.wrapping_add(rates.fast_increment);
        let slow_0 = sine_raw(self.phase_1);
        let slow_120 = sine_raw(self.phase_1.wrapping_add(ONE_THIRD));
        let slow_240 = sine_raw(self.phase_1.wrapping_add(TWO_THIRDS));
        let fast_0 = sine_raw(self.phase_2);
        let fast_120 = sine_raw(self.phase_2.wrapping_add(ONE_THIRD));
        let fast_240 = sine_raw(self.phase_2.wrapping_add(TWO_THIRDS));

        let a = depth * SLOW_SWEEP;
        let b = depth * FAST_SWEEP;
        let k = rates.to_48k;
        let tap_1 = (slow_0 * a + fast_0 * b + CENTRE) * k;
        let tap_2 = (slow_120 * a + fast_120 * b + CENTRE) * k;
        let tap_3 = (slow_240 * a + fast_240 * b + CENTRE) * k;

        let w = self.write_ptr;
        self.left.buffer[w] = left;
        self.right.buffer[w] = right;

        let wet = self.left.read(w, tap_1) * 0.33
            + self.left.read(w, tap_2) * 0.33
            + self.right.read(w, tap_3) * 0.33;
        let out_left = wet * amount + left * dry_amount;
        let wet = self.right.read(w, tap_1) * 0.33
            + self.right.read(w, tap_2) * 0.33
            + self.left.read(w, tap_3) * 0.33;
        let out_right = wet * amount + right * dry_amount;
        (out_left, out_right)
    }
}
