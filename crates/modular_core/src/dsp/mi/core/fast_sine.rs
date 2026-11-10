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
    /// `NaN` until the first render, which starts at its frequency.
    epsilon: f32,
    amplitude: f32,
}

impl Default for QuadratureOscillator {
    fn default() -> Self {
        Self {
            x: 1.0,
            y: 0.0,
            epsilon: f32::NAN,
            amplitude: 0.0,
        }
    }
}

impl QuadratureOscillator {
    /// `N` steps of the point at `frequency` cycles per step (within ±0.25),
    /// its radius `amplitude`. Frequency and radius ramp to their new values
    /// across the steps, as Plaits' interpolators do across a block, except
    /// the first frequency, which the point starts at. A negative frequency runs the recurrence's exact inverse, so the point
    /// retraces the forward path backward.
    ///
    /// For a fixed frequency the recurrence keeps x² + y² + |ε|·x·y
    /// constant, at 1 from the starting point. As the frequency ramps, the
    /// point is rescaled to bring it back to 1, so a pitch moving every
    /// sample, as under audio-rate FM, cannot pump the radius.
    #[inline(always)]
    pub fn render<const N: usize>(&mut self, frequency: f32, amplitude: f32) -> [(f32, f32); N] {
        let target = fast_2_sin(frequency);
        if self.epsilon.is_nan() {
            self.epsilon = target;
        }
        let epsilon_increment = (target - self.epsilon) / N as f32;
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
            if epsilon_increment != 0.0 {
                self.epsilon += epsilon_increment;
                let invariant = x * x + y * y + self.epsilon.abs() * x * y;
                // Near 1, as it is when the frequency moves a little each
                // step, one Newton step from 1 gives 1/√ to within
                // (invariant − 1)²; farther off, or for a degenerate point,
                // it takes the exact root.
                let error = invariant - 1.0;
                let scale = if error.abs() < 0.01 {
                    1.0 - 0.5 * error
                } else if invariant > 0.0 {
                    1.0 / invariant.sqrt()
                } else {
                    1.0
                };
                x *= scale;
                y *= scale;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Under audio-rate FM locked to the pitch, which moves the frequency
    /// every sample in step with the path, the radius stays on the ellipse
    /// the frequency allows rather than pumping, at any rate.
    #[test]
    fn radius_holds_under_audio_rate_fm() {
        for sample_rate in [44100.0f32, 48000.0, 96000.0] {
            for (ratio, depth) in [(2.0f64, 1.0f64), (1.0, 1.0), (2.0, 0.3)] {
                let hz = 130.81;
                let mut path = QuadratureOscillator::default();
                let (mut low, mut high) = (f32::MAX, 0.0f32);
                for n in 0..(sample_rate as usize * 30) {
                    let t = n as f64 / f64::from(sample_rate);
                    let modulation = depth * (std::f64::consts::TAU * ratio * hz * t).sin();
                    let f0 = (hz * (1.0 + modulation) / f64::from(sample_rate)) as f32;
                    let steps = path.render::<2>(f0 * 0.5, 1.0);
                    // The radius fades in over the first sample.
                    if n == 0 {
                        continue;
                    }
                    for (x, y) in steps {
                        let norm = x * x + y * y;
                        low = low.min(norm);
                        high = high.max(norm);
                    }
                }
                assert!(
                    low > 0.97 && high < 1.03,
                    "{sample_rate} Hz, ratio {ratio}, depth {depth}: norm² {low}–{high}"
                );
            }
        }
    }
}
