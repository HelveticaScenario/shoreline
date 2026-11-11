//! `stmlib::DelayLine` over a heap buffer sized at `init` for the sample rate.
//! The length is a power of two, so wrapping is a mask, as with stmlib's
//! fixed lengths.

#[derive(Default)]
pub struct DelayLine {
    line: Box<[f32]>,
    mask: usize,
    write_ptr: usize,
}

impl DelayLine {
    /// A silent line of at least `length` samples, rounded up to a power of
    /// two. Allocates, so it runs on the main thread.
    pub fn new(length: usize) -> Self {
        let length = length.next_power_of_two();
        Self {
            line: vec![0.0; length].into_boxed_slice(),
            mask: length - 1,
            write_ptr: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.line.len()
    }

    #[inline(always)]
    pub fn write(&mut self, sample: f32) {
        self.line[self.write_ptr] = sample;
        self.write_ptr = (self.write_ptr + self.mask) & self.mask;
    }

    #[inline(always)]
    fn at(&self, offset: usize) -> f32 {
        self.line[(self.write_ptr + offset) & self.mask]
    }

    /// The sample `delay` samples back, linearly interpolated; `delay` is
    /// non-negative.
    #[inline(always)]
    pub fn read(&self, delay: f32) -> f32 {
        let integral = delay as usize;
        let fractional = delay - integral as f32;
        let a = self.at(integral);
        let b = self.at(integral + 1);
        a + (b - a) * fractional
    }

    /// The sample `delay` samples back, Hermite-interpolated; `delay` is at
    /// least 1.
    #[inline(always)]
    pub fn read_hermite(&self, delay: f32) -> f32 {
        let integral = delay as usize;
        let f = delay - integral as f32;
        let t = self.write_ptr + integral + self.line.len();
        let xm1 = self.line[(t - 1) & self.mask];
        let x0 = self.line[t & self.mask];
        let x1 = self.line[(t + 1) & self.mask];
        let x2 = self.line[(t + 2) & self.mask];
        let c = (x1 - xm1) * 0.5;
        let v = x0 - x1;
        let w = c + v;
        let a = w + v + (x2 - x0) * 0.5;
        let b_neg = w + a;
        (((a * f) - b_neg) * f + c) * f + x0
    }

    /// A Schroeder allpass of `delay` samples and `coefficient` around the
    /// line.
    #[inline(always)]
    pub fn allpass(&mut self, sample: f32, delay: usize, coefficient: f32) -> f32 {
        let read = self.at(delay);
        let write = sample + coefficient * read;
        self.write(write);
        -write * coefficient + read
    }
}
