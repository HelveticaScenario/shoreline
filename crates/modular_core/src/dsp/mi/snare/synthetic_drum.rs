//! `plaits::SyntheticSnareDrum`, a little like a 909 snare: two coupled
//! oscillators a ratio of 1.47 apart for the body, and filtered noise for the
//! snares.

use crate::dsp::mi::core::{
    control::Ramp,
    filter::{OnePole, Svf, tan_fast},
    random::Random,
    units::semitones_to_ratio,
};

/// Per-sample constants for a sample rate, stated in Plaits' 48 kHz terms.
pub struct SyntheticRates {
    sample_rate: f32,
    /// The rate over 48 kHz.
    to_48k: f32,
    /// √`to_48k`: the gain that keeps filtered white noise's level at any rate.
    noise_gain: f32,
    /// The attack pitch sweep's 7 ms decay.
    fm_decay: f32,
    /// The filters' highest frequency, 24 kHz.
    max_frequency: f32,
    /// The share of an oscillator's increment its phase gains on each reset
    /// over Plaits', so a reset lags the crossing by 1/48000 s on average,
    /// as Plaits' does at 48 kHz.
    reset_lag: f32,
}

impl SyntheticRates {
    pub fn new(sample_rate: f32) -> Self {
        let to_48k = sample_rate / 48000.0;
        Self {
            sample_rate,
            to_48k,
            noise_gain: to_48k.sqrt(),
            fm_decay: 1.0 - 1.0 / (0.007 * sample_rate),
            max_frequency: (24000.0 / sample_rate).min(0.5),
            reset_lag: 1.0 - to_48k,
        }
    }
}

/// The drum's knobs for a sample, each 0–1.
pub struct SyntheticKnobs {
    pub accent: f32,
    pub attack_fm: f32,
    pub decay: f32,
    pub snappy: f32,
}

/// The controls derived from the decay, attack FM and snappiness.
#[derive(Default)]
struct Envelopes {
    /// The attack FM squared, as Plaits squares it.
    fm_amount: f32,
    drum_decay: f32,
    snare_decay: f32,
    /// The snares' share of the mix.
    snappy: f32,
    drum_level: f32,
    snare_level: f32,
}

/// The body's oscillator shape: a triangle, offset and soft-clipped.
#[inline(always)]
fn distorted_sine(phase: f32) -> f32 {
    let triangle = if phase < 0.5 { phase } else { 1.0 - phase } * 4.0 - 1.3;
    2.0 * triangle / (1.0 + triangle.abs())
}

pub struct SyntheticSnareDrum {
    phase: [f32; 2],
    drum_amplitude: f32,
    snare_amplitude: f32,
    fm: f32,
    /// Plaits' one-sample interpolator of the drone's level.
    sustain_gain: Ramp,
    hold_counter: i32,
    /// Whether this is an odd sample, on which alone the drum's quiet tail
    /// decays: Plaits decays it on every other sample of its blocks, so the
    /// tail below 0.03 fades at half speed.
    odd: bool,
    drum_lp: OnePole,
    snare_hp: OnePole,
    snare_lp: Svf,
    /// How far the oscillators' resets wander, from the pitch and attack FM.
    reset_noise_amount: f32,
    envelopes: Envelopes,
    /// The decay, attack FM and snappiness `envelopes` were derived from, and
    /// the pitch, snare share and attack FM the filters were set from; NaN
    /// until the first sample.
    envelope_key: [f32; 3],
    filter_key: [f32; 3],
}

impl Default for SyntheticSnareDrum {
    fn default() -> Self {
        Self {
            phase: [0.0; 2],
            drum_amplitude: 0.0,
            snare_amplitude: 0.0,
            fm: 0.0,
            sustain_gain: Ramp::default(),
            hold_counter: 0,
            odd: false,
            drum_lp: OnePole::default(),
            snare_hp: OnePole::default(),
            snare_lp: Svf::default(),
            reset_noise_amount: 0.0,
            envelopes: Envelopes::default(),
            envelope_key: [f32::NAN; 3],
            filter_key: [f32::NAN; 3],
        }
    }
}

impl SyntheticSnareDrum {
    #[inline(always)]
    fn controls(&mut self, f0: f32, knobs: &SyntheticKnobs, rates: &SyntheticRates) {
        let key = [knobs.decay, knobs.attack_fm, knobs.snappy];
        if key.map(f32::to_bits) != self.envelope_key.map(f32::to_bits) {
            self.envelope_key = key;
            let SyntheticKnobs {
                decay,
                attack_fm,
                snappy,
                ..
            } = *knobs;
            let decay_xt = decay * (1.0 + decay * (decay - 1.0));
            let fm_amount = attack_fm * attack_fm;
            let sample_rate = rates.sample_rate;
            let drum_decay = 1.0
                - 1.0 / (0.015 * sample_rate)
                    * semitones_to_ratio(-decay_xt * 72.0 - fm_amount * 12.0 + snappy * 7.0);
            let snare_decay =
                1.0 - 1.0 / (0.01 * sample_rate) * semitones_to_ratio(-decay * 60.0 - snappy * 7.0);
            let snappy = (snappy * 1.1 - 0.05).clamp(0.0, 1.0);
            self.envelopes = Envelopes {
                fm_amount,
                drum_decay,
                snare_decay,
                snappy,
                drum_level: (1.0 - snappy).sqrt(),
                snare_level: snappy.sqrt(),
            };
        }
        let Envelopes {
            fm_amount, snappy, ..
        } = self.envelopes;
        let key = [f0, snappy, fm_amount];
        if key.map(f32::to_bits) != self.filter_key.map(f32::to_bits) {
            self.filter_key = key;
            let snare_f_min = (10.0 * f0).min(rates.max_frequency);
            let snare_f_max = (35.0 * f0).min(rates.max_frequency);
            self.snare_hp.set_g(tan_fast(snare_f_min));
            self.snare_lp
                .set_g_q(tan_fast(snare_f_max), 0.5 + 2.0 * snappy);
            // Capped at 24 kHz, so a pitch jumping every sample cannot pump
            // its state.
            self.drum_lp
                .set_g(tan_fast((3.0 * f0).min(rates.max_frequency)));
            let amount = ((0.125 - f0 * rates.to_48k) * 8.0).clamp(0.0, 1.0);
            self.reset_noise_amount = amount * amount * fm_amount;
        }
    }

    /// One sample at `f0` cycles per sample. `sustain` drones instead of
    /// sounding on strikes; `trigger` strikes the drum this sample.
    #[inline(always)]
    pub fn render(
        &mut self,
        sustain: bool,
        trigger: bool,
        f0: f32,
        knobs: &SyntheticKnobs,
        random: &mut Random,
        rates: &SyntheticRates,
    ) -> f32 {
        self.controls(f0, knobs, rates);
        if trigger {
            self.drum_amplitude = 0.3 + 0.7 * knobs.accent;
            self.snare_amplitude = self.drum_amplitude;
            self.fm = 1.0;
            self.phase = [0.0; 2];
            // The snare holds for 40 to 70 ms before it decays.
            self.hold_counter = ((0.04 + knobs.decay * 0.03) * rates.sample_rate) as i32;
        }
        self.sustain_gain.set(knobs.accent * knobs.decay, Some(1.0));
        let odd = self.odd;
        self.odd = !odd;

        let e = &self.envelopes;
        if sustain {
            self.snare_amplitude = self.sustain_gain.next_value();
            self.drum_amplitude = self.snare_amplitude;
            self.fm = 0.0;
        } else {
            if self.drum_amplitude > 0.03 || odd {
                self.drum_amplitude *= e.drum_decay;
            }
            if self.hold_counter > 0 {
                self.hold_counter -= 1;
            } else {
                self.snare_amplitude *= e.snare_decay;
            }
            self.fm *= rates.fm_decay;
        }

        // The 909's oscillators reset each other, which intermodulates them.
        let amount = self.reset_noise_amount;
        let mut reset_noise = 0.0;
        reset_noise += if self.phase[0] > 0.5 { -1.0 } else { 1.0 };
        reset_noise += if self.phase[1] > 0.5 { -1.0 } else { 1.0 };
        let reset_noise = reset_noise * (amount * 0.025);

        let f = f0 * (1.0 + e.fm_amount * (4.0 * self.fm));
        let increments = [f, f * 1.47];
        for (phase, increment) in self.phase.iter_mut().zip(increments) {
            *phase += increment;
            if amount > 0.1 {
                // Past the reset point the phase folds back below 0 by its
                // overshoot.
                if *phase >= 1.0 + reset_noise {
                    *phase = (1.0 - *phase) + rates.reset_lag * increment;
                }
            } else if *phase >= 1.0 {
                // An increment past a cycle, at full attack FM on the highest
                // notes, wraps by whole cycles.
                *phase -= phase.floor();
            }
        }

        let mut drum = -0.1;
        drum += distorted_sine(self.phase[0]) * 0.60;
        drum += distorted_sine(self.phase[1]) * 0.25;
        drum *= self.drum_amplitude * e.drum_level;
        let drum = self.drum_lp.low_pass(drum);

        // Only the noise's swing scales with the rate; its mean of 0.5 sets
        // the high-pass's start-up bump, which is the same at any rate.
        let noise = 0.5 + (random.float() - 0.5) * rates.noise_gain;
        let snare = self.snare_lp.low_pass(noise);
        let snare = self.snare_hp.high_pass(snare);
        let snare = (snare + 0.1) * (self.snare_amplitude + self.fm) * e.snare_level;

        snare + drum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Struck often at full attack FM, from the lowest pitch to the highest
    /// at any engine rate, the oscillators' phases stay within the reset
    /// point's swing and two increments of 0–1 (at 48 kHz or the engine rate,
    /// whichever is larger), and within 0–1 where the resets don't wander
    /// (above about 4 kHz).
    #[test]
    fn phases_stay_bounded() {
        for sample_rate in [8000.0, 48000.0, 192000.0] {
            let rates = SyntheticRates::new(sample_rate);
            for hz in [1.0, 200.0, 3000.0, 4500.0, 8400.0, 20000.0] {
                let f0 = (hz / sample_rate).min(0.5);
                let knobs = SyntheticKnobs {
                    accent: 1.0,
                    attack_fm: 1.0,
                    decay: 0.5,
                    snappy: 0.5,
                };
                let mut drum = SyntheticSnareDrum::default();
                let mut random = Random::new(1);
                // The largest increment, at the attack's 5 times the pitch.
                let increment = 5.0 * 1.47 * f0.max(hz / 48000.0);
                for n in 0..20000 {
                    drum.render(false, n % 500 == 0, f0, &knobs, &mut random, &rates);
                    for phase in drum.phase {
                        let bounds = if drum.reset_noise_amount > 0.1 {
                            -0.15 - 2.0 * increment..1.05 + 2.0 * increment
                        } else {
                            0.0..1.0
                        };
                        assert!(
                            bounds.contains(&phase),
                            "{hz} Hz at {sample_rate} Hz: phase {phase}"
                        );
                    }
                }
            }
        }
    }
}
