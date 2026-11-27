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
    /// Holds the fed-back signal within full scale.
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

    /// The sample to record for `input`, mixed with the fed-back signal. Only
    /// the fed-back signal is limited and soft-clipped, so the input records
    /// at any level. Below full scale this matches the hardware, which drives
    /// the input into the soft clipper with the feedback: the input's gain
    /// rises with `feedback` by the clipper's pre-gain.
    #[inline]
    pub fn record(&mut self, input: f32) -> f32 {
        let feedback = self.feedback.next();
        let k = self.soft_limit_gain;
        if k != 0.0 {
            let fed = self
                .limiter
                .process_mono(self.previous * feedback * (self.max_gain / k));
            input * (1.0 + (k - 1.0) * feedback) + soft_clip(fed * k) * feedback
        } else {
            let fed = self.previous * feedback * feedback * self.max_gain;
            input + self.limiter.process_mono(fed)
        }
    }

    /// Filter the played signal into the next sample's feedback.
    #[inline]
    pub fn feed_back(&mut self, played: f32) {
        let (_, hp) = self.dc_blocker.process(played);
        self.previous += self.lp_coefficient * (hp - self.previous);
    }
}
