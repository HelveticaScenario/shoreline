//! Short-time Fourier transform for two sides, one sample in and one out per
//! call: every hop a side's last frame is windowed, transformed, handed to
//! the caller as a spectrum to rewrite, and overlap-added back.

use std::sync::{Arc, Mutex};

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// Frames overlap by this factor: the hop is `size / OVERLAP`.
const OVERLAP: usize = 4;

/// Plans and window for one frame size.
struct Shared {
    size: usize,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    /// Square root of a periodic Hann window, applied before the forward
    /// transform and again after the inverse.
    window: Box<[f32]>,
    /// Undoes the unnormalised inverse and the summed squared windows.
    norm: f32,
}

/// The plans for `size`, built once and shared by every instance. The cache
/// keeps a reference forever, so dropping an [`Stft`] never frees them.
fn shared_for(size: usize) -> Arc<Shared> {
    static CACHE: Mutex<Vec<Arc<Shared>>> = Mutex::new(Vec::new());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(shared) = cache.iter().find(|s| s.size == size) {
        return shared.clone();
    }
    let mut planner = FftPlanner::<f32>::new();
    let shared = Arc::new(Shared {
        size,
        forward: planner.plan_fft_forward(size),
        inverse: planner.plan_fft_inverse(size),
        window: (0..size)
            .map(|i| (std::f32::consts::PI * i as f32 / size as f32).sin())
            .collect(),
        norm: 1.0 / (size as f32 * (OVERLAP / 2) as f32),
    });
    cache.push(shared.clone());
    shared
}

/// Frame size for an engine rate: about 85 ms, so the frequency resolution
/// and the latency stay close across rates.
fn frame_size(sample_rate: f32) -> usize {
    if sample_rate <= 50_000.0 {
        4096
    } else if sample_rate <= 100_000.0 {
        8192
    } else {
        16384
    }
}

#[derive(Default)]
struct Side {
    /// The last `size` input samples; `pos` is the oldest.
    input: Vec<f32>,
    /// Overlap-added output, read and cleared at `pos`.
    output: Vec<f32>,
    pos: usize,
    /// Samples until this side's next frame.
    countdown: usize,
}

/// Empty until [`Stft::allocate`]; an empty transform outputs silence.
#[derive(Default)]
pub struct Stft {
    shared: Option<Arc<Shared>>,
    sides: [Side; 2],
    frame: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
}

impl Stft {
    /// Size every buffer for `sample_rate`. Main thread only.
    pub fn allocate(&mut self, sample_rate: f32) {
        let size = frame_size(sample_rate);
        let shared = shared_for(size);
        let hop = size / OVERLAP;
        let scratch = shared
            .forward
            .get_inplace_scratch_len()
            .max(shared.inverse.get_inplace_scratch_len());
        self.frame = vec![Complex::default(); size];
        self.scratch = vec![Complex::default(); scratch];
        for (i, side) in self.sides.iter_mut().enumerate() {
            side.input = vec![0.0; size];
            side.output = vec![0.0; size];
            side.pos = 0;
            // The sides transform half a hop apart, so no sample pays for
            // both.
            side.countdown = hop + i * hop / 2;
        }
        self.shared = Some(shared);
    }

    /// Samples in a frame, which is also the latency in samples.
    pub fn size(&self) -> usize {
        self.frame.len()
    }

    /// Samples between two frames of one side.
    pub fn hop(&self) -> usize {
        self.size() / OVERLAP
    }

    /// Bins handed to `rewrite`: DC to Nyquist inclusive.
    pub fn bins(&self) -> usize {
        self.size() / 2 + 1
    }

    /// Push `x` into `side` and pop its output from one frame earlier. When
    /// the side's frame falls due, `rewrite` gets its spectrum and leaves the
    /// spectrum to resynthesise.
    #[inline]
    pub fn tick(&mut self, side: usize, x: f32, rewrite: impl FnOnce(&mut [Complex<f32>])) -> f32 {
        let Self {
            shared,
            sides,
            frame,
            scratch,
        } = self;
        let Some(shared) = shared else {
            return 0.0;
        };
        let size = shared.size;
        let side = &mut sides[side];

        side.input[side.pos] = x;
        let out = side.output[side.pos];
        side.output[side.pos] = 0.0;
        side.pos = (side.pos + 1) % size;

        side.countdown -= 1;
        if side.countdown == 0 {
            side.countdown = size / OVERLAP;
            let (newest, oldest) = side.input.split_at(side.pos);
            for ((bin, x), w) in frame
                .iter_mut()
                .zip(oldest.iter().chain(newest))
                .zip(shared.window.iter())
            {
                *bin = Complex::new(x * w, 0.0);
            }
            shared.forward.process_with_scratch(frame, scratch);

            let half = size / 2;
            rewrite(&mut frame[..=half]);
            for k in 1..half {
                frame[size - k] = frame[k].conj();
            }
            shared.inverse.process_with_scratch(frame, scratch);

            let (head, tail) = side.output.split_at_mut(side.pos);
            for ((out, bin), w) in tail
                .iter_mut()
                .chain(head)
                .zip(frame.iter())
                .zip(shared.window.iter())
            {
                *out += bin.re * w * shared.norm;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_spectrum_returns_the_input_one_frame_late() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            let mut stft = Stft::default();
            stft.allocate(sample_rate);
            let size = stft.size();
            let input = |n: usize| ((n * 37 % 101) as f32 / 50.0 - 1.0) * 3.0;
            for n in 0..size * 6 {
                for side in 0..2 {
                    let out = stft.tick(side, input(n + side), |_| {});
                    // The first frames of a cold start are only partly
                    // overlapped.
                    if n >= size * 2 {
                        let expected = input(n + side - size);
                        assert!(
                            (out - expected).abs() < 1e-3,
                            "{sample_rate} Hz, side {side}, sample {n}: {out} vs {expected}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn an_unallocated_transform_is_silent() {
        let mut stft = Stft::default();
        assert_eq!(stft.tick(0, 1.0, |_| {}), 0.0);
    }
}
