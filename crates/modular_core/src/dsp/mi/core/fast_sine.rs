//! `plaits::FastSineOscillator`: a sine and cosine from the "magic circle"
//! recurrence, without a table.

/// `FastSineOscillator::Fast2Sin`: 2 sin(π f), from a cubic that keeps the
/// pitch within a few cents from 16 Hz to 16 kHz at 48 kHz.
#[inline(always)]
pub fn fast_2_sin(f: f32) -> f32 {
    let f_pi = f * std::f32::consts::PI;
    f_pi * (2.0 - (2.0 * 0.96 / 6.0) * f_pi * f_pi)
}

/// `stmlib::fast_rsqrt_carmack`: 1/√`x` to about 0.2%.
#[inline(always)]
fn fast_rsqrt(x: f32) -> f32 {
    let y = f32::from_bits(0x5f37_59df - (x.to_bits() >> 1));
    y * (1.5 - (x * 0.5 * y * y))
}

/// The oscillator in Plaits' quadrature mode: a point circling the origin,
/// returned as its two coordinates.
#[derive(Clone, Copy)]
pub struct QuadratureOscillator {
    x: f32,
    y: f32,
    epsilon: f32,
    amplitude: f32,
}

impl Default for QuadratureOscillator {
    fn default() -> Self {
        Self {
            x: 1.0,
            y: 0.0,
            epsilon: 0.0,
            amplitude: 0.0,
        }
    }
}

impl QuadratureOscillator {
    /// `N` steps of the point at `frequency` cycles per step (within ±0.25),
    /// its radius `amplitude`. Frequency and radius ramp to their new values
    /// across the steps, as Plaits' interpolators do across a block. A
    /// negative frequency runs the recurrence's exact inverse, so the point
    /// retraces the forward path backward.
    #[inline(always)]
    pub fn render<const N: usize>(&mut self, frequency: f32, amplitude: f32) -> [(f32, f32); N] {
        let epsilon_increment = (fast_2_sin(frequency) - self.epsilon) / N as f32;
        let amplitude_increment = (amplitude - self.amplitude) / N as f32;
        let (mut x, mut y) = (self.x, self.y);
        // The recurrence drifts off the unit circle; pull it back when it
        // strays far.
        let norm = x * x + y * y;
        if norm <= 0.5 || norm >= 2.0 {
            let scale = fast_rsqrt(norm);
            x *= scale;
            y *= scale;
        }
        let out = std::array::from_fn(|_| {
            self.epsilon += epsilon_increment;
            self.amplitude += amplitude_increment;
            if self.epsilon >= 0.0 {
                x += self.epsilon * y;
                y -= self.epsilon * x;
            } else {
                y -= self.epsilon * x;
                x += self.epsilon * y;
            }
            (x * self.amplitude, y * self.amplitude)
        });
        self.x = x;
        self.y = y;
        out
    }
}
