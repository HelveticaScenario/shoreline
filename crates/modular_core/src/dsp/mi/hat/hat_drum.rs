//! `plaits::HiHat`, a model of the 808 hi-hat: metallic noise through a
//! band-pass, blended with clocked noise, then a VCA and a high-pass.

use crate::dsp::mi::core::{
    control::Ramp,
    dsp::decay_per_sample,
    filter::{Svf, tan_accurate},
    random::Random,
    units::semitones_to_ratio,
};

use super::noise::{MetallicNoise, RingModNoise, SquareNoise, ring_frequencies};

/// Per-sample constants for a sample rate, stated in Plaits' 48 kHz terms.
pub struct HatRates {
    /// The rate over 48 kHz.
    pub(super) to_48k: f32,
    /// The band-pass's lowest frequency, 150 Hz.
    filter_base: f32,
    /// The filters' highest frequency, 16 kHz.
    max_cutoff: f32,
    /// The square oscillators' highest frequency, 23.952 kHz.
    pub(super) max_square: f32,
    /// The noise clock's highest rate, 24 kHz.
    max_noise_clock: f32,
    /// `RingModNoise`'s frequencies in cycles per sample.
    pub(super) ring_frequencies: [f32; 6],
}

impl HatRates {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            to_48k: sample_rate / 48000.0,
            filter_base: 150.0 / sample_rate,
            max_cutoff: (16000.0 / sample_rate).min(0.499),
            max_square: (23952.0 / sample_rate).min(0.499),
            max_noise_clock: (24000.0 / sample_rate).min(0.5),
            ring_frequencies: ring_frequencies(sample_rate),
        }
    }
}

/// A hi-hat's noise source, VCA and filter, as Plaits' template parameters
/// choose them.
pub trait Model {
    type Noise: MetallicNoise;
    /// Whether the band-pass's resonance rises with the tone.
    const RESONANCE: bool;
    /// Whether the envelope cuts off faster once below 1/2.
    const TWO_STAGE: bool;

    fn vca(s: f32, gain: f32) -> f32;
}

/// `$mi.hiHat`: `HiHat<SquareNoise, SwingVCA, true, false>`.
pub struct SquareHat;

impl Model for SquareHat {
    type Noise = SquareNoise;
    const RESONANCE: bool = true;
    const TWO_STAGE: bool = false;

    /// A VCA that swings its input's positive half 40 times harder than its
    /// negative half, then saturates it.
    #[inline(always)]
    fn vca(s: f32, gain: f32) -> f32 {
        let s = s * if s > 0.0 { 4.0 } else { 0.1 };
        let s = s / (1.0 + s.abs());
        (s + 0.1) * gain
    }
}

/// `$mi.ringHat`: `HiHat<RingModNoise, LinearVCA, false, true>`.
pub struct RingHat;

impl Model for RingHat {
    type Noise = RingModNoise;
    const RESONANCE: bool = false;
    const TWO_STAGE: bool = true;

    #[inline(always)]
    fn vca(s: f32, gain: f32) -> f32 {
        s * gain
    }
}

/// The hat's knobs for a sample, each 0–1.
pub struct HatKnobs {
    pub accent: f32,
    pub tone: f32,
    pub decay: f32,
    pub noisiness: f32,
}

pub struct HiHat<M: Model> {
    envelope: f32,
    noise_clock: f32,
    noise_sample: f32,
    /// Plaits' one-sample interpolator of the drone's level.
    sustain_gain: Ramp,
    metallic: M::Noise,
    coloration: Svf,
    hpf: Svf,
    envelope_decay: f32,
    /// The envelope's decay below 1/2, on a two-stage envelope.
    cut_decay: f32,
    /// The clocked noise's rate in cycles per sample, and its share of the
    /// mix.
    noise_f: f32,
    noisiness: f32,
    /// The inputs the derived controls above were last computed from; NaN
    /// until the first sample.
    decay_key: f32,
    tone_key: f32,
    pitch_key: f32,
    noise_key: [f32; 2],
}

impl<M: Model> Default for HiHat<M> {
    fn default() -> Self {
        Self {
            envelope: 0.0,
            noise_clock: 0.0,
            noise_sample: 0.0,
            sustain_gain: Ramp::default(),
            metallic: M::Noise::default(),
            coloration: Svf::default(),
            hpf: Svf::default(),
            envelope_decay: 0.0,
            cut_decay: 0.0,
            noise_f: 0.0,
            noisiness: 0.0,
            decay_key: f32::NAN,
            tone_key: f32::NAN,
            pitch_key: f32::NAN,
            noise_key: [f32::NAN; 2],
        }
    }
}

impl<M: Model> HiHat<M> {
    #[inline(always)]
    fn controls(&mut self, f0: f32, knobs: &HatKnobs, rates: &HatRates) {
        let decay = knobs.decay;
        if decay.to_bits() != self.decay_key.to_bits() {
            self.decay_key = decay;
            let envelope_decay = 1.0 - 0.003 * semitones_to_ratio(-decay * 84.0);
            let cut_decay = 1.0 - 0.0025 * semitones_to_ratio(-decay * 36.0);
            self.envelope_decay = decay_per_sample(envelope_decay, rates.to_48k);
            self.cut_decay = decay_per_sample(cut_decay, rates.to_48k);
        }
        let tone = knobs.tone;
        if tone.to_bits() != self.tone_key.to_bits() {
            self.tone_key = tone;
            let cutoff =
                (rates.filter_base * semitones_to_ratio(tone * 72.0)).clamp(0.0, rates.max_cutoff);
            let g = tan_accurate(cutoff);
            let q = if M::RESONANCE { 3.0 + 3.0 * tone } else { 1.0 };
            self.coloration.set_g_q(g, q);
            self.hpf.set_g_q(g, 0.5);
        }
        if f0.to_bits() != self.pitch_key.to_bits() {
            self.pitch_key = f0;
            self.metallic.set_pitch(2.0 * f0, rates);
        }
        let key = [f0, knobs.noisiness];
        if key.map(f32::to_bits) != self.noise_key.map(f32::to_bits) {
            self.noise_key = key;
            let noisiness = knobs.noisiness * knobs.noisiness;
            self.noisiness = noisiness;
            self.noise_f =
                (f0 * (16.0 + 16.0 * (1.0 - noisiness))).clamp(0.0, rates.max_noise_clock);
        }
    }

    /// One sample at `f0` cycles per sample. `sustain` drones instead of
    /// sounding on strikes; `trigger` strikes the hat this sample.
    #[inline(always)]
    pub fn render(
        &mut self,
        sustain: bool,
        trigger: bool,
        f0: f32,
        knobs: &HatKnobs,
        random: &mut Random,
        rates: &HatRates,
    ) -> f32 {
        self.controls(f0, knobs, rates);
        if trigger {
            self.envelope = (1.5 + 0.5 * (1.0 - knobs.decay)) * (0.3 + 0.7 * knobs.accent);
        }
        let metallic = self.coloration.band_pass(self.metallic.next());

        // Not part of the 808: noise clocked at 16 to 32 times the pitch,
        // blended into the oscillators.
        self.noise_clock += self.noise_f;
        if self.noise_clock >= 1.0 {
            self.noise_clock -= 1.0;
            self.noise_sample = random.float() - 0.5;
        }
        let noise = metallic + self.noisiness * (self.noise_sample - metallic);

        self.sustain_gain.set(knobs.accent * knobs.decay, Some(1.0));
        self.envelope *= if self.envelope > 0.5 || !M::TWO_STAGE {
            self.envelope_decay
        } else {
            self.cut_decay
        };
        let gain = if sustain {
            self.sustain_gain.next_value()
        } else {
            self.envelope
        };
        self.hpf.high_pass(M::vca(noise, gain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOBS: HatKnobs = HatKnobs {
        accent: 1.0,
        tone: 0.5,
        decay: 1.0,
        noisiness: 0.0,
    };

    /// Noisiness slows the noise clock from 32 to 16 times the pitch.
    #[test]
    fn noise_clock_follows_noisiness() {
        let rates = HatRates::new(48000.0);
        let f0 = 300.0 / 48000.0;
        for (noisiness, ratio) in [(0.0, 32.0), (1.0, 16.0)] {
            let mut hat = HiHat::<SquareHat>::default();
            let knobs = HatKnobs { noisiness, ..KNOBS };
            hat.render(false, false, f0, &knobs, &mut Random::new(1), &rates);
            assert!((hat.noise_f / f0 - ratio).abs() < 1e-3, "{noisiness}");
        }
    }

    /// A full strike at the longest decay starts at 1.5, and the ring hat's
    /// envelope chokes once below a third of that, while the 808 hat's does
    /// not.
    #[test]
    fn ring_hat_chokes_below_a_third_of_a_full_strike() {
        let rates = HatRates::new(48000.0);
        let mut ring = HiHat::<RingHat>::default();
        let mut square = HiHat::<SquareHat>::default();
        let mut random = Random::new(1);
        ring.render(false, true, 0.01, &KNOBS, &mut random, &rates);
        square.render(false, true, 0.01, &KNOBS, &mut random, &rates);
        assert!((ring.envelope / ring.envelope_decay - 1.5).abs() < 1e-6);
        assert!(ring.cut_decay < ring.envelope_decay);
        for _ in 0..100_000 {
            let (before, square_before) = (ring.envelope, square.envelope);
            ring.render(false, false, 0.01, &KNOBS, &mut random, &rates);
            square.render(false, false, 0.01, &KNOBS, &mut random, &rates);
            let factor = if before > 0.5 {
                ring.envelope_decay
            } else {
                ring.cut_decay
            };
            assert_eq!(ring.envelope, before * factor);
            assert_eq!(square.envelope, square_before * square.envelope_decay);
        }
        assert!(ring.envelope < 0.01);
    }
}
