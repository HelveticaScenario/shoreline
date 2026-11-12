//! `plaits::SyntheticBassDrum`: a sine with a pitch envelope, a click and a
//! burst of noise, each through a transistor-like VCA.

use crate::dsp::mi::core::{
    control::Ramp,
    dsp::{one_pole, per_sample, slope},
    filter::{Svf, tan_fast},
    random::Random,
    sine::sine,
    units::semitones_to_ratio,
};

/// Per-sample constants for a sample rate, stated in Plaits' 48 kHz terms.
pub struct SyntheticRates {
    sample_rate: f32,
    /// The rate over 48 kHz.
    to_48k: f32,
    /// √`to_48k`: the gain that keeps filtered white noise's level at any rate.
    noise_gain: f32,
    /// The body's 1 ms hold at full level after a strike.
    body_pulse: i32,
    /// The 1.3 ms the phase is held at its peak after a strike.
    fm_pulse: i32,
    /// The click and noise's 5 ms decay.
    transient_decay: f32,
    /// The envelopes' smoothing, 0.1 per sample at 48 kHz.
    envelope_lp: f32,
    /// The phase noise's low-pass, 0.002 per sample at 48 kHz.
    phase_noise_lp: f32,
    /// The attack noise's band-pass, 0.05 and 0.005 per sample at 48 kHz.
    noise_lp: f32,
    noise_hp: f32,
    /// The click's slew, rising at 0.5 and falling at 0.1 per sample at
    /// 48 kHz, and its high-pass, 0.04.
    click_rise: f32,
    click_fall: f32,
    click_hp: f32,
    /// The click's 5 kHz low-pass.
    click_g: f32,
    /// The highest phase increment, 24 kHz.
    max_increment: f32,
}

impl SyntheticRates {
    pub fn new(sample_rate: f32) -> Self {
        let to_48k = sample_rate / 48000.0;
        Self {
            sample_rate,
            to_48k,
            noise_gain: to_48k.sqrt(),
            body_pulse: (sample_rate * 0.001f32) as i32,
            fm_pulse: (sample_rate * 0.0013f32) as i32,
            transient_decay: 1.0 - 1.0 / (0.005 * sample_rate),
            envelope_lp: per_sample(0.1, to_48k),
            phase_noise_lp: per_sample(0.002, to_48k),
            noise_lp: per_sample(0.05, to_48k),
            noise_hp: per_sample(0.005, to_48k),
            click_rise: per_sample(0.5, to_48k),
            click_fall: per_sample(0.1, to_48k),
            click_hp: per_sample(0.04, to_48k),
            click_g: tan_fast(5000.0 / sample_rate),
            max_increment: (24000.0 / sample_rate).min(0.5),
        }
    }
}

/// The drum's knobs for a sample, each 0–1.
pub struct SyntheticKnobs {
    pub accent: f32,
    pub tone: f32,
    pub decay: f32,
    pub dirtiness: f32,
    pub fm_envelope: f32,
    pub fm_decay: f32,
}

/// The controls derived from the pitch, tone, decay and FM decay.
#[derive(Default)]
struct Controls {
    tone_f: f32,
    body_env_decay: f32,
    fm_decay: f32,
}

/// The phase, bent by `phase_noise` and folded toward a soft triangle by
/// `dirtiness`, read as a sine.
#[inline(always)]
fn distorted_sine(phase: f32, phase_noise: f32, dirtiness: f32) -> f32 {
    let phase = phase + phase_noise * dirtiness;
    let phase = phase - (phase as i32) as f32;
    let triangle = if phase < 0.5 { phase } else { 1.0 - phase } * 4.0 - 1.0;
    let sine_like = 2.0 * triangle / (1.0 + triangle.abs());
    let clean_sine = sine(phase + 0.75);
    sine_like + (1.0 - dirtiness) * (clean_sine - sine_like)
}

/// A transistor VCA's soft gain curve, with its control leaking through.
#[inline(always)]
fn transistor_vca(s: f32, gain: f32) -> f32 {
    let s = (s - 0.6) * gain;
    3.0 * s / (2.0 + s.abs()) + gain * 0.3
}

/// The click: a slewed step, high-passed and then low-passed at 5 kHz.
#[derive(Default)]
struct Click {
    lp: f32,
    hp: f32,
    filter: Svf,
}

impl Click {
    #[inline(always)]
    fn process(&mut self, x: f32, rates: &SyntheticRates) -> f32 {
        slope(&mut self.lp, x, rates.click_rise, rates.click_fall);
        one_pole(&mut self.hp, self.lp, rates.click_hp);
        self.filter.low_pass(self.lp - self.hp)
    }
}

/// The attack noise: white noise band-passed by two one-poles.
#[derive(Default)]
struct AttackNoise {
    lp: f32,
    hp: f32,
}

impl AttackNoise {
    #[inline(always)]
    fn render(&mut self, random: &mut Random, rates: &SyntheticRates) -> f32 {
        // Only the noise's swing scales with the rate; its mean of 0.5 sets
        // the start-up bump, which is the same at any rate.
        let noise = 0.5 + (random.float() - 0.5) * rates.noise_gain;
        one_pole(&mut self.lp, noise, rates.noise_lp);
        one_pole(&mut self.hp, self.lp, rates.noise_hp);
        self.lp - self.hp
    }
}

pub struct SyntheticBassDrum {
    /// Plaits' one-sample interpolators of the pitch and the drone's level,
    /// which land a hair off their targets.
    f0: Ramp,
    sustain_gain: Ramp,
    phase: f32,
    phase_noise: f32,
    fm: f32,
    fm_lp: f32,
    body_env: f32,
    body_env_lp: f32,
    transient_env: f32,
    transient_env_lp: f32,
    tone_lp: f32,
    click: Click,
    noise: AttackNoise,
    body_env_pulse_width: i32,
    fm_pulse_width: i32,
    controls: Controls,
    /// The pitch, tone, decay and FM decay `controls` were derived from; NaN
    /// until the first sample.
    key: [f32; 4],
}

impl SyntheticBassDrum {
    /// A drum at rest, with its click filter set for `rates`.
    pub fn new(rates: &SyntheticRates) -> Self {
        let mut click = Click::default();
        click.filter.set_g_q(rates.click_g, 2.0);
        Self {
            f0: Ramp::default(),
            sustain_gain: Ramp::default(),
            phase: 0.0,
            phase_noise: 0.0,
            fm: 0.0,
            fm_lp: 0.0,
            body_env: 0.0,
            body_env_lp: 0.0,
            transient_env: 0.0,
            transient_env_lp: 0.0,
            tone_lp: 0.0,
            click,
            noise: AttackNoise::default(),
            body_env_pulse_width: 0,
            fm_pulse_width: 0,
            controls: Controls::default(),
            key: [f32::NAN; 4],
        }
    }

    /// `decay` and `fm_decay` are squared, as Plaits squares them.
    #[inline(always)]
    fn controls(&mut self, f0: f32, tone: f32, decay: f32, fm_decay: f32, rates: &SyntheticRates) {
        let key = [f0, tone, decay, fm_decay];
        if key.map(f32::to_bits) == self.key.map(f32::to_bits) {
            return;
        }
        self.key = key;
        let sample_rate = rates.sample_rate;
        let tone_f = (4.0 * (f0 * rates.to_48k) * semitones_to_ratio(tone * 108.0)).min(1.0);
        self.controls = Controls {
            tone_f: per_sample(tone_f, rates.to_48k),
            body_env_decay: 1.0 - 1.0 / (0.02 * sample_rate) * semitones_to_ratio(-decay * 60.0),
            fm_decay: 1.0 - 1.0 / (0.008 * (1.0 + fm_decay * 4.0) * sample_rate),
        };
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
        let decay = knobs.decay * knobs.decay;
        let fm_decay = knobs.fm_decay * knobs.fm_decay;
        self.f0.set(f0, Some(1.0));
        let dirtiness = knobs.dirtiness * (1.0 - 8.0 * (f0 * rates.to_48k)).max(0.0);
        self.controls(f0, knobs.tone, decay, fm_decay, rates);
        let transient_level = knobs.tone;

        if trigger {
            self.fm = 1.0;
            self.body_env = 0.3 + 0.7 * knobs.accent;
            self.transient_env = self.body_env;
            self.body_env_pulse_width = rates.body_pulse;
            self.fm_pulse_width = rates.fm_pulse;
        }
        self.sustain_gain.set(knobs.accent * decay, Some(1.0));

        one_pole(
            &mut self.phase_noise,
            (random.float() - 0.5) * rates.noise_gain,
            rates.phase_noise_lp,
        );

        let mut mix = 0.0;
        let c = &self.controls;
        if sustain {
            self.phase += self.f0.next_value();
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
            let body = distorted_sine(self.phase, self.phase_noise, dirtiness);
            mix -= transistor_vca(body, self.sustain_gain.next_value());
        } else {
            if self.fm_pulse_width > 0 {
                self.fm_pulse_width -= 1;
                self.phase = 0.25;
            } else {
                self.fm *= c.fm_decay;
                let fm = 1.0 + knobs.fm_envelope * 3.5 * self.fm_lp;
                self.phase += (self.f0.next_value() * fm).min(rates.max_increment);
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                }
            }

            if self.body_env_pulse_width > 0 {
                self.body_env_pulse_width -= 1;
            } else {
                self.body_env *= c.body_env_decay;
                self.transient_env *= rates.transient_decay;
            }

            one_pole(&mut self.body_env_lp, self.body_env, rates.envelope_lp);
            one_pole(
                &mut self.transient_env_lp,
                self.transient_env,
                rates.envelope_lp,
            );
            one_pole(&mut self.fm_lp, self.fm, rates.envelope_lp);

            let body = distorted_sine(self.phase, self.phase_noise, dirtiness);
            let step = if self.body_env_pulse_width > 0 {
                0.0
            } else {
                1.0
            };
            let transient = self.click.process(step, rates) + self.noise.render(random, rates);

            mix -= transistor_vca(body, self.body_env_lp);
            mix -= transient * self.transient_env_lp * transient_level;
        }

        one_pole(&mut self.tone_lp, mix, c.tone_f);
        self.tone_lp
    }
}
