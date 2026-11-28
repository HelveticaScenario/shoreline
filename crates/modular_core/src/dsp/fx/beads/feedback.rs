//! Beads' recording path: the input plus limited, soft-clipped feedback
//! that is written to the buffer, and the filtering that turns the played
//! signal into the next sample's feedback.

use super::buffer::Resolution;
use super::primitives::{Limiter, Ramp, Svf, soft_clip};

#[derive(Default, Clone, Copy)]
pub struct FeedbackPath {
    feedback: Ramp,
    previous: f32,
    dc_blocker: Svf,
    /// Holds the recording within full scale while feedback pushes it past.
    limiter: Limiter,
    /// Low-pass coefficient on the feedback signal.
    lp_coefficient: f32,
    /// Pre-gain into the feedback soft clipper; 0 disables it.
    soft_limit_gain: f32,
    /// Feedback gain at full `feedback`.
    max_gain: f32,
}

impl FeedbackPath {
    pub fn reset(&mut self, sample_rate: f32, resolution: Resolution, max_gain: f32) {
        self.feedback.reset(0.0);
        self.previous = 0.0;
        self.dc_blocker = Svf::default();
        self.dc_blocker.set_f_q(40.0 / sample_rate, 0.5);
        self.limiter.init(1.0 / sample_rate);
        (self.lp_coefficient, self.soft_limit_gain) = match resolution {
            Resolution::MuLaw8 => (0.75, 1.75),
            Resolution::Linear12 => (0.9, 1.3),
            Resolution::Linear16 => (0.999, 0.0),
        };
        self.max_gain = max_gain;
    }

    pub fn start_block(&mut self, feedback: f32, block_size: usize) {
        self.feedback.start(feedback, block_size);
    }

    /// The sample to record for `input`, mixed with the fed-back signal.
    /// Within full scale this is the hardware's path: the limiter holds the
    /// sum within full scale, and at 12-bit and µ-law the soft clipper
    /// saturates it as `feedback` rises. The input's excess past full scale
    /// bypasses both, so it records at any level.
    #[inline]
    pub fn record(&mut self, input: f32) -> f32 {
        let feedback = self.feedback.next();
        let within = input.clamp(-1.0, 1.0);
        let excess = input - within;
        let k = self.soft_limit_gain;
        let mut s = within;
        if k != 0.0 {
            s += self.previous * feedback * (self.max_gain / k);
        } else {
            s += self.previous * feedback * feedback * self.max_gain;
        }
        s = self.limiter.process_mono(s);
        if k != 0.0 {
            s = within + (soft_clip(s * k) - within) * feedback;
        }
        s + excess
    }

    /// Filter the played signal into the next sample's feedback.
    #[inline]
    pub fn feed_back(&mut self, played: f32) {
        let (_, hp) = self.dc_blocker.process(played);
        self.previous += self.lp_coefficient * (hp - self.previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hardware's recording path, which also limits the input.
    fn hardware(path: &mut FeedbackPath, input: f32) -> f32 {
        let feedback = path.feedback.next();
        let k = path.soft_limit_gain;
        let mut s = input;
        if k != 0.0 {
            s += path.previous * feedback * (path.max_gain / k);
        } else {
            s += path.previous * feedback * feedback * path.max_gain;
        }
        s = path.limiter.process_mono(s);
        if k != 0.0 {
            s = input + (soft_clip(s * k) - input) * feedback;
        }
        s
    }

    fn paths() -> impl Iterator<Item = FeedbackPath> {
        [
            Resolution::MuLaw8,
            Resolution::Linear12,
            Resolution::Linear16,
        ]
        .into_iter()
        .map(|resolution| {
            let mut path = FeedbackPath::default();
            path.reset(48000.0, resolution, 3.0);
            path.start_block(0.8, 12);
            path
        })
    }

    #[test]
    fn within_full_scale_records_as_the_hardware() {
        // A loud input fed back hard, so the limiter and soft clipper work.
        for mut path in paths() {
            let mut reference = path;
            for n in 0..4800 {
                let x = (n as f32 * 0.05).sin() * 0.95;
                let ours = path.record(x);
                assert_eq!(ours, hardware(&mut reference, x), "sample {n}");
                path.feed_back(ours);
                reference.feed_back(ours);
            }
        }
    }
}
