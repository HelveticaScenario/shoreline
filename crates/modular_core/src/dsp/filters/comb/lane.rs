//! One lane of the comb: a delay line tuned to a pitch, fed back through a
//! one-pole damping lowpass and a soft clip.

use crate::dsp::utils::{
    C4_HZ_F32, CatmullRom, changed, delay_line::DelayLine, sanitize, saturation::tanh_pade,
};
use crate::types::SwapGlide;

/// Lowest pitch the line is long enough for, about 20 Hz: the bottom of
/// `freq`'s range.
pub(super) const MIN_FREQ_VOCT: f32 = -3.71;

/// Shortest delay in samples: the cubic read needs a sample on each side.
const MIN_DELAY: f32 = 2.0;

/// Decades `1 - g` falls across `feedback`'s range when the gain per pass
/// is the same on every pitch: `1 - g = 10^(-|feedback| · FEEDBACK_DECADES
/// / 5)`, so the loop gain is 0.995 at ±5, a ring of seconds at bass pitches.
const FEEDBACK_DECADES: f32 = 2.3;

/// Seconds the teeth take to fade by 60 dB at `feedback` ±5 when every pitch
/// rings for the same time: long enough
/// to sound endless.
const LONGEST_RING: f32 = 20.0;

/// Growth of the ring time per unit of `feedback`: about 40 ms at 1, 1 s at
/// 3 and 4.4 s at 4.
const RING_GROWTH: f32 = 4.5;

/// Damping lowpass cutoff scale in Hz; `damp` 1 puts it near 10 kHz.
const DAMP_HZ: f32 = 20_000.0;

/// Octaves the damping cutoff falls per unit of `damp`.
const DAMP_OCTAVES: f32 = 1.6;

/// Level in volts the fed-back signal soft-clips toward, so a loop near
/// unity fed a matching pitch saturates instead of growing without bound.
const FEEDBACK_LIMIT: f32 = 5.0;

/// Gain per pass for `feedback`, the same on every pitch, so low notes,
/// which pass less often, ring longer. Signed like `feedback`: 0 at 0,
/// rising toward 1.
pub(super) fn pass_gain(feedback: f32) -> f32 {
    let amount = feedback.abs().min(5.0);
    (1.0 - 10f32.powf(-amount * FEEDBACK_DECADES / 5.0)).copysign(feedback)
}

/// Seconds the teeth take to fade by 60 dB at `feedback` when every pitch
/// rings for the same time: 0 at 0, rising
/// exponentially to `LONGEST_RING` at ±5.
pub(super) fn ring_time(feedback: f32) -> f32 {
    let amount = feedback.abs().min(5.0);
    LONGEST_RING * (RING_GROWTH.powf(amount) - 1.0) / (RING_GROWTH.powf(5.0) - 1.0)
}

/// Gain per pass around a loop `loop_samples` long that fades by 60 dB in
/// `feedback`'s ring time, signed like `feedback`. Shorter loops pass more often, so
/// they lose less on each pass and every pitch rings for the same time.
pub(super) fn even_ring_gain(feedback: f32, loop_samples: f32, sample_rate: f32) -> f32 {
    let ring = ring_time(feedback);
    if ring == 0.0 {
        return 0.0;
    }
    10f32
        .powf(-3.0 * loop_samples / (sample_rate * ring))
        .copysign(feedback)
}

/// Pole `b` of the damping lowpass `y += (1 - b)(x - y)` for `damp` at
/// `sample_rate`: 0 (no damping) at 0, darker as it rises.
pub(super) fn damping_pole(damp: f32, sample_rate: f32) -> f32 {
    let damp = damp.clamp(0.0, 5.0);
    if damp == 0.0 {
        return 0.0;
    }
    let fall = (-DAMP_OCTAVES * damp).exp2();
    let cutoff = DAMP_HZ * fall / (1.0 - fall);
    (-std::f32::consts::TAU * cutoff / sample_rate).exp()
}

/// The line's delay in samples that puts the loop's lowest resonance at
/// `hz`, and the loop's whole length at `hz` once the damping lowpass's
/// phase lag is added back. A negative loop gain inverts each pass, so the
/// line is half as long and the comb keeps its pitch with only odd
/// harmonics. Taking the lag off the line keeps the comb in tune.
fn tuned_delay(hz: f32, negative: bool, pole: f32, sample_rate: f32, max_delay: f32) -> (f32, f32) {
    let w = std::f32::consts::TAU * hz / sample_rate;
    let lag = (pole * w.sin()).atan2(1.0 - pole * w.cos()) / w;
    let period = if negative { 0.5 } else { 1.0 } * sample_rate / hz;
    let delay = (period - lag).clamp(MIN_DELAY, max_delay);
    (delay, delay + lag)
}

/// Per-lane settings derived from the controls; `NaN` keys = never computed.
#[derive(Clone, Copy)]
struct Tuning {
    /// `freq`, `feedback`, `damp` and `evenRing` as 0 or 1.
    key: [f32; 4],
    delay: f32,
    gain: f32,
    input_gain: f32,
    pole: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            key: [f32::NAN; 4],
            delay: MIN_DELAY,
            gain: 0.0,
            input_gain: 1.0,
            pole: 0.0,
        }
    }
}

/// One sample's controls for a lane, before their swap glides.
#[derive(Clone, Copy)]
pub(super) struct Controls {
    pub freq: f32,
    pub feedback: f32,
    pub damp: f32,
}

/// Swap glides for a lane's controls, in `Controls` order.
#[derive(Clone, Copy, Default)]
pub(super) struct Glides {
    pub freq: SwapGlide,
    pub feedback: SwapGlide,
    pub damp: SwapGlide,
}

/// Fixed per-module numbers every lane reads.
#[derive(Clone, Copy, Default)]
pub(super) struct Rates {
    pub sample_rate: f32,
    /// Longest delay the line can be read at.
    pub max_delay: f32,
}

impl Rates {
    pub(super) fn new(sample_rate: f32) -> Self {
        let min_hz = C4_HZ_F32 * MIN_FREQ_VOCT.exp2();
        Self {
            sample_rate,
            max_delay: (sample_rate / min_hz).ceil(),
        }
    }

    /// Line capacity: the longest delay plus the cubic read's far sample.
    pub(super) fn capacity(&self) -> usize {
        self.max_delay as usize + 2
    }
}

#[derive(Default)]
pub(super) struct CombLane {
    line: DelayLine,
    tuning: Tuning,
    /// The damping lowpass's state.
    damped: f32,
    pub glides: Glides,
}

impl crate::types::SeedSide for CombLane {
    fn seed_side(&mut self, left: &Self) {
        let Self {
            line: _,
            tuning: _,
            damped: _,
            glides: _,
        } = left;
        self.line.seed_side(&left.line);
        self.tuning = left.tuning;
        self.damped = left.damped;
        self.glides = left.glides;
    }
}

impl CombLane {
    pub(super) fn allocate(&mut self, rates: &Rates) {
        self.line = DelayLine::new(rates.capacity());
    }

    fn tune(&mut self, freq: f32, feedback: f32, damp: f32, even_ring: bool, rates: &Rates) {
        let key = [freq, feedback, damp, f32::from(u8::from(even_ring))];
        if !key
            .iter()
            .zip(&self.tuning.key)
            .any(|(&a, &b)| changed(a, b))
        {
            return;
        }
        let sr = rates.sample_rate;
        let pole = damping_pole(damp, sr);
        let hz = C4_HZ_F32 * freq.max(MIN_FREQ_VOCT).exp2();
        let (delay, loop_samples) = tuned_delay(hz, feedback < 0.0, pole, sr, rates.max_delay);
        let gain = if even_ring {
            even_ring_gain(feedback, loop_samples, sr)
        } else {
            pass_gain(feedback)
        };
        self.tuning = Tuning {
            key,
            delay,
            gain,
            input_gain: (1.0 - gain * gain).sqrt(),
            pole,
        };
    }

    /// The line `delay` samples back, read before this sample is written.
    #[inline(always)]
    fn read(&self, delay: f32) -> f32 {
        let whole = delay as usize;
        let t = delay - whole as f32;
        let p = [
            self.line.read(whole - 2),
            self.line.read(whole - 1),
            self.line.read(whole),
            self.line.read(whole + 1),
        ];
        CatmullRom::new(p).at(t)
    }

    /// One sample. Both render paths call this, so they cannot drift.
    #[inline(always)]
    pub(super) fn step(
        &mut self,
        input: f32,
        controls: Controls,
        marks: [crate::types::GlideMark; 3],
        even_ring: bool,
        rates: &Rates,
    ) -> f32 {
        let sr = rates.sample_rate;
        let freq = self
            .glides
            .freq
            .follow(sanitize(controls.freq), marks[0], sr);
        let feedback = self
            .glides
            .feedback
            .follow(sanitize(controls.feedback), marks[1], sr);
        let damp = self
            .glides
            .damp
            .follow(sanitize(controls.damp), marks[2], sr);
        self.tune(freq, feedback, damp, even_ring, rates);
        let t = self.tuning;
        let delayed = self.read(t.delay);
        self.damped = sanitize(delayed + t.pole * (self.damped - delayed));
        let fed = FEEDBACK_LIMIT * tanh_pade(t.gain * self.damped / FEEDBACK_LIMIT);
        let out = sanitize(sanitize(input) * t.input_gain + fed);
        self.line.write(out);
        out
    }
}
