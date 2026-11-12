//! `plaits::AnalogSnareDrum`, a model of the 808 snare: a pulse rings five
//! resonant shell modes, mixed with band-passed noise for the snares.

use crate::dsp::mi::core::{
    control::Ramp,
    dsp::{decay_per_sample, one_pole, per_sample_lag, soft_clip},
    filter::{Svf, tan_fast},
    random::Random,
    sine::SineOscillator,
    units::semitones_to_ratio,
};

const MODES: usize = 5;

/// The shell modes' frequencies over the pitch.
const MODE_RATIOS: [f32; MODES] = [1.0, 2.0, 3.18, 4.16, 5.62];

/// Per-sample constants for a sample rate, stated in Plaits' 48 kHz terms.
pub struct AnalogRates {
    /// The rate over 48 kHz.
    to_48k: f32,
    /// √`to_48k`: the gain that keeps filtered white noise's level at any rate.
    noise_gain: f32,
    /// The trigger pulse's 1 ms.
    trigger_pulse: i32,
    /// The trigger pulse's 0.1 ms tail.
    pulse_decay: f32,
    /// The pulse shaping's one-pole, 0.75 per sample at 48 kHz, whose
    /// difference from the pulse strikes the lowest mode.
    pulse_filter: f32,
    /// The shell modes' and noise filter's highest frequency, 23.952 kHz.
    max_frequency: f32,
}

impl AnalogRates {
    pub fn new(sample_rate: f32) -> Self {
        let to_48k = sample_rate / 48000.0;
        Self {
            to_48k,
            noise_gain: to_48k.sqrt(),
            trigger_pulse: (1.0e-3f32 * sample_rate) as i32,
            pulse_decay: decay_per_sample(1.0 - 1.0 / (0.1e-3f32 * 48000.0), to_48k),
            pulse_filter: per_sample_lag(0.75, to_48k),
            max_frequency: (23952.0 / sample_rate).min(0.499),
        }
    }
}

/// The drum's knobs for a sample, each 0–1.
pub struct AnalogKnobs {
    pub accent: f32,
    pub tone: f32,
    pub decay: f32,
    pub snappy: f32,
}

/// The controls derived from the decay and snappiness.
#[derive(Default)]
struct NoiseControls {
    noise_envelope_decay: f32,
    exciter_leak: f32,
    /// The snares' share of the mix.
    snappy: f32,
}

pub struct AnalogSnareDrum {
    pulse_remaining: i32,
    pulse: f32,
    pulse_height: f32,
    pulse_lp: f32,
    noise_envelope: f32,
    /// Plaits' one-sample interpolator of the drone's level.
    sustain_gain: Ramp,
    resonators: [Svf; MODES],
    noise_filter: Svf,
    /// Ring in the resonators' place while the drum drones.
    oscillators: [SineOscillator; MODES],
    /// The shell modes' frequencies.
    f: [f32; MODES],
    gains: [f32; MODES],
    noise: NoiseControls,
    /// The pitch and decay `f` and the filters were set from, the tone
    /// `gains` were derived from, and the decay and snappiness `noise` was
    /// derived from; NaN until the first sample.
    filter_key: [f32; 2],
    tone_key: f32,
    noise_key: [f32; 2],
}

impl Default for AnalogSnareDrum {
    fn default() -> Self {
        Self {
            pulse_remaining: 0,
            pulse: 0.0,
            pulse_height: 0.0,
            pulse_lp: 0.0,
            noise_envelope: 0.0,
            sustain_gain: Ramp::default(),
            resonators: [Svf::default(); MODES],
            noise_filter: Svf::default(),
            oscillators: [SineOscillator::default(); MODES],
            f: [0.0; MODES],
            gains: [0.0; MODES],
            noise: NoiseControls::default(),
            filter_key: [f32::NAN; 2],
            tone_key: f32::NAN,
            noise_key: [f32::NAN; 2],
        }
    }
}

/// The shell modes' gains for `tone`: two 808-style modes below 2/3, then
/// three more rising in turn.
fn mode_gains(tone: f32) -> [f32; MODES] {
    let mut gain = [0.0; MODES];
    if tone < 0.666667 {
        let tone = tone * 1.5;
        gain[0] = 1.5 + (1.0 - tone) * (1.0 - tone) * 4.5;
        gain[1] = 2.0 * tone + 0.15;
    } else {
        let mut tone = (tone - 0.666667) * 3.0;
        gain[0] = 1.5 - tone * 0.5;
        gain[1] = 2.15 - tone * 0.7;
        for g in &mut gain[2..] {
            *g = tone;
            tone *= tone;
        }
    }
    gain
}

impl AnalogSnareDrum {
    #[inline(always)]
    fn controls(&mut self, f0: f32, knobs: &AnalogKnobs, rates: &AnalogRates) {
        let decay = knobs.decay;
        if [f0, decay].map(f32::to_bits) != self.filter_key.map(f32::to_bits) {
            self.filter_key = [f0, decay];
            let decay_xt = decay * (1.0 + decay * (decay - 1.0));
            let q = 2000.0 * semitones_to_ratio(decay_xt * 84.0);
            for (i, resonator) in self.resonators.iter_mut().enumerate() {
                let f = (f0 * MODE_RATIOS[i]).min(rates.max_frequency);
                self.f[i] = f;
                let q = if i == 0 { q } else { q * 0.25 };
                resonator.set_g_q(tan_fast(f), 1.0 + (f * rates.to_48k) * q);
            }
            let f_noise = (f0 * 16.0).clamp(0.0, rates.max_frequency);
            self.noise_filter
                .set_g_q(tan_fast(f_noise), 1.0 + (f_noise * rates.to_48k) * 1.5);
        }
        if knobs.tone.to_bits() != self.tone_key.to_bits() {
            self.tone_key = knobs.tone;
            self.gains = mode_gains(knobs.tone);
        }
        let snappy = knobs.snappy;
        if [decay, snappy].map(f32::to_bits) != self.noise_key.map(f32::to_bits) {
            self.noise_key = [decay, snappy];
            let noise_envelope_decay =
                1.0 - 0.0017 * semitones_to_ratio(-decay * (50.0 + snappy * 10.0));
            self.noise = NoiseControls {
                noise_envelope_decay: decay_per_sample(noise_envelope_decay, rates.to_48k),
                exciter_leak: snappy * (2.0 - snappy) * 0.1,
                snappy: (snappy * 1.1 - 0.05).clamp(0.0, 1.0),
            };
        }
    }

    /// One sample at `f0` cycles per sample. `sustain` drones instead of
    /// ringing on strikes; `trigger` strikes the drum this sample.
    #[inline(always)]
    pub fn render(
        &mut self,
        sustain: bool,
        trigger: bool,
        f0: f32,
        knobs: &AnalogKnobs,
        random: &mut Random,
        rates: &AnalogRates,
    ) -> f32 {
        self.controls(f0, knobs, rates);
        if trigger {
            self.pulse_remaining = rates.trigger_pulse;
            self.pulse_height = 3.0 + 7.0 * knobs.accent;
            self.noise_envelope = 2.0;
        }
        self.sustain_gain.set(knobs.accent * knobs.decay, Some(1.0));

        // Q45 / Q46
        let pulse = if self.pulse_remaining > 0 {
            self.pulse_remaining -= 1;
            self.pulse = if self.pulse_remaining > 0 {
                self.pulse_height
            } else {
                self.pulse_height - 1.0
            };
            self.pulse
        } else {
            self.pulse *= rates.pulse_decay;
            self.pulse
        };
        let sustain_gain = self.sustain_gain.next_value();

        // R189 / C57 / R190 + C58 / C59 / R197 / R196 / IC14
        one_pole(&mut self.pulse_lp, pulse, rates.pulse_filter);

        let mut shell = 0.0;
        let leak = self.noise.exciter_leak;
        for i in 0..MODES {
            let mode = if sustain {
                self.oscillators[i].next_sine(self.f[i]) * sustain_gain * 0.25
            } else {
                let excitation = if i == 0 {
                    (pulse - self.pulse_lp) + 0.006 * pulse
                } else {
                    0.026 * pulse
                };
                self.resonators[i].band_pass(excitation) + excitation * leak
            };
            shell += self.gains[i] * mode;
        }
        let shell = soft_clip(shell);

        // C56 / R194 / Q48 / C54 / R188 / D54. The half-wave rectified
        // noise's mean of 0.25, which each strike's envelope turns into a
        // bump through the band-pass, is the same at any rate; only its
        // swing scales with the rate.
        let noise = (2.0 * random.float() - 1.0).max(0.0);
        let noise = 0.25 + (noise - 0.25) * rates.noise_gain;
        self.noise_envelope *= self.noise.noise_envelope_decay;
        let envelope = if sustain {
            sustain_gain
        } else {
            self.noise_envelope
        };
        let snappy = self.noise.snappy;
        let noise = noise * (envelope * snappy * 2.0);

        // C66 / R201 / C67 / R202 / R203 / Q49
        let noise = self.noise_filter.band_pass(noise);

        // IC13
        noise + shell * (1.0 - snappy)
    }
}
