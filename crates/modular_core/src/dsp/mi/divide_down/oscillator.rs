//! `plaits::StringSynthOscillator`: four BLEP saws an octave apart, divided
//! down from one phase, as a string synth's divide-down organ derives its
//! footages.

use crate::dsp::mi::core::polyblep::{edge_time, next_blep_sample, this_blep_sample};

/// Saw levels for 8′, 4′, 2′ and 1′ (the saws at 1, 2, 4 and 8 times the
/// lowest pitch), from a registration of seven amplitudes (saw and square
/// partials) shifted up by `shift` octave pairs.
pub fn saw_gains(registration: &[f32; 7], shift: usize, gain: f32) -> [f32; 4] {
    let mut r = [0.0; 7];
    r[shift..].copy_from_slice(&registration[..7 - shift]);
    [
        (r[0] + 2.0 * r[1]) * gain,
        (r[2] - r[1] + 2.0 * r[3]) * gain,
        (r[4] - r[3] + 2.0 * r[5]) * gain,
        (r[6] - r[5]) * gain,
    ]
}

/// Plaits' octave shift for very high pitches: `frequency` (cycles per sample
/// of the eight-segment phase) halved until it is at most 0.5, with the
/// registration shifted two slots per halving. `None` when the pitch is out
/// of reach.
pub fn shift_down(frequency: f32) -> Option<(f32, usize)> {
    let mut frequency = frequency;
    let mut shift = 0;
    while frequency.abs() > 0.5 {
        shift += 2;
        frequency *= 0.5;
    }
    (shift < 8).then_some((frequency, shift))
}

/// The phase runs across eight segments; each saw resets at the segments its
/// period ends on.
#[derive(Clone, Copy, Default)]
pub struct DivideDownOscillator {
    phase: f32,
    next_sample: f32,
    segment: i32,
}

impl DivideDownOscillator {
    /// One sample at `frequency` (within ±0.5 segments per sample; negative
    /// runs the saws backward, their edges mirrored) with the four saw
    /// levels `gains`.
    #[inline(always)]
    pub fn step(&mut self, frequency: f32, gains: [f32; 4]) -> f32 {
        let [saw_8, saw_4, saw_2, saw_1] = gains;
        let mut this_sample = self.next_sample;
        let mut next_sample = 0.0;
        self.phase += frequency;
        if self.phase < 0.0 {
            self.phase += 8.0;
            // A phase a hair below 0 wraps to 8, which is 0.
            if self.phase >= 8.0 {
                self.phase = 0.0;
            }
        }
        let mut next_segment = self.phase as i32;
        if next_segment != self.segment {
            // The boundary crossed and how long ago: the next segment's start
            // going forward, the last segment's going backward.
            let (boundary, since, sign) = if frequency >= 0.0 {
                if next_segment == 8 {
                    self.phase -= 8.0;
                    next_segment = 0;
                }
                let since = self.phase - next_segment as f32;
                (next_segment, edge_time(since, frequency), -1.0)
            } else {
                let boundary = self.segment;
                let edge = if boundary == 0 { 8.0 } else { boundary as f32 };
                (boundary, edge_time(edge - self.phase, -frequency), 1.0)
            };
            let mut discontinuity = 0.0;
            if boundary == 0 {
                discontinuity += saw_8;
            }
            if boundary & 3 == 0 {
                discontinuity += saw_4;
            }
            if boundary & 1 == 0 {
                discontinuity += saw_2;
            }
            discontinuity += saw_1;
            let discontinuity = sign * discontinuity;
            if discontinuity != 0.0 {
                this_sample += this_blep_sample(since) * discontinuity;
                next_sample += next_blep_sample(since) * discontinuity;
            }
        }
        self.segment = next_segment;

        let segment = self.segment;
        next_sample += (self.phase - 4.0) * saw_8 * 0.125;
        next_sample += (self.phase - (segment & 4) as f32 - 2.0) * saw_4 * 0.25;
        next_sample += (self.phase - (segment & 6) as f32 - 1.0) * saw_2 * 0.5;
        next_sample += (self.phase - (segment & 7) as f32 - 0.5) * saw_1;
        self.next_sample = next_sample;
        2.0 * this_sample
    }
}
