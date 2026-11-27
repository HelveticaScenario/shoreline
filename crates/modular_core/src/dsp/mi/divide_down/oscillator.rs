//! `plaits::StringSynthOscillator`: four BLEP saws an octave apart, divided
//! down from one phase, as a string synth's divide-down organ derives its
//! footages. A phase offset moves the shared phase through time as extra
//! frequency, so every saw shifts with it and every edge stays band-limited.

use crate::dsp::mi::core::{
    phase::limit_frequency,
    phase_offset::Travel,
    polyblep::{edge_time, next_blep_sample, this_blep_sample},
};

/// Plaits' `ComputeRegistration`: the seven amplitudes (saw and square
/// partials, interleaved) that `registration` (0–1) blends to across `table`;
/// the seventh is always 0.
pub fn registration(table: &[[f32; 6]], registration: f32) -> [f32; 7] {
    let position = registration * (table.len() as f32 - 1.001);
    let integral = position as usize;
    let fractional = position - integral as f32;
    let mut amplitudes = [0.0; 7];
    for (i, amplitude) in amplitudes[..6].iter_mut().enumerate() {
        let a = table[integral][i];
        let b = table[integral + 1][i];
        *amplitude = a + (b - a) * fractional;
    }
    amplitudes
}

/// Plaits' 48 kHz pitch of `f0` (cycles per sample at the rate whose ratio
/// to 48 kHz is `to_48k`), or this rate's when that is nearer Nyquist.
#[inline(always)]
fn speed(f0: f32, to_48k: f32) -> f32 {
    f0.abs().max(f0.abs() * to_48k)
}

/// The level that fades a voice at `f0` out toward Plaits' 6 kHz, and toward
/// this rate's equivalent when that is lower.
#[inline(always)]
pub fn fade(f0: f32, to_48k: f32) -> f32 {
    (4.0 - speed(limit_frequency(f0, 0.5), to_48k) * 32.0).clamp(0.0, 1.0)
}

/// Saw levels for 8′, 4′, 2′ and 1′ (the saws at 1, 2, 4 and 8 times the
/// lowest pitch), from a registration of seven amplitudes (saw and square
/// partials) shifted up by `shift` octave pairs.
fn saw_gains(registration: &[f32; 7], shift: usize, gain: f32) -> [f32; 4] {
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
/// of the eight-segment phase) halved, and the registration shifted two
/// slots, for each halving that brings `speed` (the same pitch in Plaits'
/// 48 kHz terms, or this rate's when nearer Nyquist) to at most 0.5. `None`
/// when the pitch is out of reach.
fn shift_down(frequency: f32, speed: f32) -> Option<(f32, usize)> {
    let mut frequency = frequency;
    let mut speed = speed;
    let mut shift = 0;
    while speed > 0.5 && shift < 8 {
        shift += 2;
        speed *= 0.5;
        frequency *= 0.5;
    }
    (shift < 8).then_some((frequency, shift))
}

/// The fastest the phase runs, in segments per sample, while it travels to an
/// offset: the most `shift_down` leaves it.
const MAX_SEGMENTS: f32 = 0.5;

/// The phase runs across eight segments; each saw resets at the segments its
/// period ends on.
#[derive(Clone, Copy, Default)]
struct DivideDownOscillator {
    phase: f32,
    next_sample: f32,
    segment: i32,
}

impl DivideDownOscillator {
    /// The four saws at the current phase, before their edges' corrections.
    #[inline(always)]
    fn naive(&self, gains: [f32; 4]) -> f32 {
        let [saw_8, saw_4, saw_2, saw_1] = gains;
        let segment = self.segment;
        (self.phase - 4.0) * saw_8 * 0.125
            + (self.phase - (segment & 4) as f32 - 2.0) * saw_4 * 0.25
            + (self.phase - (segment & 6) as f32 - 1.0) * saw_2 * 0.5
            + (self.phase - (segment & 7) as f32 - 0.5) * saw_1
    }

    /// Redraws the pending sample as the saws at levels `gains`, for a jump
    /// in the levels (an octave shift): the edge correction it carries
    /// belongs to the old levels' waveform, which the new one does not
    /// continue.
    fn redraw(&mut self, gains: [f32; 4]) {
        self.next_sample = self.naive(gains);
    }

    /// One sample at `frequency` (within ±0.5 segments per sample; negative
    /// runs the saws backward, their edges mirrored) with the four saw
    /// levels `gains`.
    #[inline(always)]
    fn step(&mut self, frequency: f32, gains: [f32; 4]) -> f32 {
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

/// A divide-down oscillator driven as `StringSynthOscillator::Render` drives
/// it: shifted down octaves at very high pitches, its saw levels drawn from a
/// registration and a gain.
#[derive(Clone, Copy, Default)]
pub struct DivideDownVoice {
    oscillator: DivideDownOscillator,
    /// The saw levels the oscillator last drew with.
    gains: [f32; 4],
    /// The phase's travel toward the offset, in cycles of the eight-segment
    /// phase.
    offset: Travel,
    /// The registration key, octave shift and gain `gains` were computed
    /// for, as bits; `None` before the first sample.
    gains_key: Option<(u32, usize, u32)>,
}

impl DivideDownVoice {
    /// Starts a fresh voice `offset` (in `[0, 1)`) of a cycle into its
    /// eight-segment phase, every saw where it would be had they started
    /// together.
    pub fn seed(&mut self, offset: f32) {
        self.oscillator.phase = offset * 8.0;
        self.oscillator.segment = self.oscillator.phase as i32;
        self.offset.seed(offset);
    }

    /// One sample at `f0` cycles per sample (at the rate whose ratio to
    /// 48 kHz is `to_48k`) with `registration`, which `registration_key`
    /// identifies, at `gain`. The phase travels to `offset` (in `[0, 1)`, in
    /// cycles of the eight-segment phase), so a voice that moves to an offset
    /// plays as one seeded there does.
    #[inline(always)]
    pub fn step(
        &mut self,
        f0: f32,
        to_48k: f32,
        registration: &[f32; 7],
        registration_key: u32,
        gain: f32,
        offset: f32,
    ) -> f32 {
        let f0 = limit_frequency(f0, 0.5);
        let Some((frequency, shift)) = shift_down(f0 * 8.0, speed(f0, to_48k) * 8.0) else {
            return 0.0;
        };
        let key = (registration_key, shift, gain.to_bits());
        if self.gains_key != Some(key) {
            let gains = saw_gains(registration, shift, gain);
            if self
                .gains_key
                .is_some_and(|(_, last_shift, _)| last_shift != shift)
            {
                self.oscillator.redraw(gains);
            }
            self.gains = gains;
            self.gains_key = Some(key);
        }
        let shift = self
            .offset
            .step(offset, 1, frequency * 0.125, MAX_SEGMENTS * 0.125);
        let frequency = frequency + shift * 8.0;
        self.oscillator.step(frequency, self.gains)
    }
}
