//! `plaits::AnalogBassDrum`, a model of the 808 bass drum circuit, and
//! `plaits::Overdrive`, which Plaits runs it through.

use crate::dsp::mi::core::{
    control::Ramp,
    dsp::{decay_per_sample, one_pole, per_sample, soft_clip},
    filter::{Svf, tan_dirty},
    sine::SineOscillator,
    units::semitones_to_ratio,
};

/// Per-sample constants for a sample rate, stated in Plaits' 48 kHz terms.
pub struct AnalogRates {
    /// The rate over 48 kHz.
    to_48k: f32,
    /// The trigger pulse's 1 ms.
    trigger_pulse: i32,
    /// The attack FM pulse's 6 ms.
    fm_pulse: i32,
    /// The trigger pulse's 0.2 ms tail.
    pulse_decay: f32,
    /// The pulse shaping's 0.1 ms time constant.
    pulse_filter: f32,
    /// The retrigger pulse's 50 ms tail.
    retrig_decay: f32,
    /// The resonator's highest frequency, 19.2 kHz.
    max_frequency: f32,
}

impl AnalogRates {
    pub fn new(sample_rate: f32) -> Self {
        let to_48k = sample_rate / 48000.0;
        Self {
            to_48k,
            trigger_pulse: (1.0e-3f32 * sample_rate) as i32,
            fm_pulse: (6.0e-3f32 * sample_rate) as i32,
            pulse_decay: decay_per_sample(1.0 - 1.0 / (0.2e-3f32 * 48000.0), to_48k),
            pulse_filter: per_sample(1.0 / (0.1e-3f32 * 48000.0), to_48k),
            retrig_decay: decay_per_sample(1.0 - 1.0 / (0.05f32 * 48000.0), to_48k),
            max_frequency: (19200.0 / sample_rate).min(0.4),
        }
    }
}

/// The drum's knobs for a sample, each 0–1.
pub struct AnalogKnobs {
    pub accent: f32,
    pub tone: f32,
    pub decay: f32,
    pub attack_fm: f32,
    pub self_fm: f32,
}

/// The controls derived from the pitch, tone and decay.
#[derive(Default)]
struct Controls {
    /// The excitation's gain, which keeps the hit's level across pitches.
    scale: f32,
    /// The resonator's Q per cycle per sample at 48 kHz.
    q: f32,
    tone_f: f32,
    exciter_leak: f32,
}

/// The diode clamping the pulse shaper's negative swing.
#[inline(always)]
fn diode(x: f32) -> f32 {
    if x >= 0.0 {
        x
    } else {
        let x = x * 2.0;
        0.7 * x / (1.0 + x.abs())
    }
}

pub struct AnalogBassDrum {
    pulse_remaining: i32,
    fm_pulse_remaining: i32,
    pulse: f32,
    pulse_height: f32,
    pulse_lp: f32,
    fm_pulse_lp: f32,
    retrig_pulse: f32,
    lp_out: f32,
    tone_lp: f32,
    /// Plaits' one-sample interpolator of the drone's level, which lands a
    /// hair off its target.
    sustain_gain: Ramp,
    resonator: Svf,
    /// Rings in the resonator's place while the drum drones.
    oscillator: SineOscillator,
    controls: Controls,
    /// The pitch, tone and decay `controls` were derived from; NaN until the
    /// first sample.
    key: [f32; 3],
}

impl Default for AnalogBassDrum {
    fn default() -> Self {
        Self {
            pulse_remaining: 0,
            fm_pulse_remaining: 0,
            pulse: 0.0,
            pulse_height: 0.0,
            pulse_lp: 0.0,
            fm_pulse_lp: 0.0,
            retrig_pulse: 0.0,
            lp_out: 0.0,
            tone_lp: 0.0,
            sustain_gain: Ramp::default(),
            resonator: Svf::default(),
            oscillator: SineOscillator::default(),
            controls: Controls::default(),
            key: [f32::NAN; 3],
        }
    }
}

impl AnalogBassDrum {
    #[inline(always)]
    fn controls(&mut self, f0: f32, tone: f32, decay: f32, rates: &AnalogRates) {
        let key = [f0, tone, decay];
        if key.map(f32::to_bits) == self.key.map(f32::to_bits) {
            return;
        }
        self.key = key;
        let f0 = f0 * rates.to_48k;
        let tone_f = (4.0 * f0 * semitones_to_ratio(tone * 108.0)).min(1.0);
        self.controls = Controls {
            scale: 0.001 / f0,
            q: 1500.0 * semitones_to_ratio(decay * 80.0),
            tone_f: per_sample(tone_f, rates.to_48k),
            exciter_leak: 0.08 * (tone + 0.25),
        };
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
        rates: &AnalogRates,
    ) -> f32 {
        self.controls(f0, knobs.tone, knobs.decay, rates);
        if trigger {
            self.pulse_remaining = rates.trigger_pulse;
            self.fm_pulse_remaining = rates.fm_pulse;
            self.pulse_height = 3.0 + 7.0 * knobs.accent;
            self.lp_out = 0.0;
        }
        self.sustain_gain.set(knobs.accent * knobs.decay, Some(1.0));

        // Q39 / Q40
        let mut pulse = if self.pulse_remaining > 0 {
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
        if sustain {
            pulse = 0.0;
        }

        // C40 / R163 / R162 / D83
        one_pole(&mut self.pulse_lp, pulse, rates.pulse_filter);
        let pulse = diode((pulse - self.pulse_lp) + pulse * 0.044);

        // Q41 / Q42
        let mut fm_pulse = 0.0;
        if self.fm_pulse_remaining > 0 {
            self.fm_pulse_remaining -= 1;
            fm_pulse = 1.0;
            // C39 / C52
            self.retrig_pulse = if self.fm_pulse_remaining > 0 {
                0.0
            } else {
                -0.8
            };
        } else {
            // C39 / R161
            self.retrig_pulse *= rates.retrig_decay;
        }
        if sustain {
            fm_pulse = 0.0;
        }
        one_pole(&mut self.fm_pulse_lp, fm_pulse, rates.pulse_filter);

        // Q43 and R170 leakage
        let punch = 0.7 + diode(10.0 * self.lp_out - 1.0);

        // Q43 / R165
        let attack_fm = self.fm_pulse_lp * 1.7 * knobs.attack_fm;
        let self_fm = punch * 0.08 * knobs.self_fm;
        let f = (f0 * (1.0 + attack_fm + self_fm)).clamp(0.0, rates.max_frequency);

        let c = &self.controls;
        let resonator_out = if sustain {
            let (sin, cos) = self.oscillator.next(f, self.sustain_gain.next_value());
            self.lp_out = cos;
            sin
        } else {
            self.resonator
                .set_g_q(tan_dirty(f), 1.0 + c.q * (f * rates.to_48k));
            let (bp, lp) = self
                .resonator
                .process((pulse - self.retrig_pulse * 0.2) * c.scale);
            self.lp_out = lp;
            bp
        };

        one_pole(
            &mut self.tone_lp,
            pulse * c.exciter_leak + resonator_out,
            c.tone_f,
        );
        self.tone_lp
    }
}

/// Plaits' overdrive: a soft clipper whose output gain keeps the level as the
/// drive rises.
pub struct Overdrive {
    /// Plaits' one-sample interpolators of the two gains.
    pre_gain: Ramp,
    post_gain: Ramp,
    gains: (f32, f32),
    /// The drive `gains` were derived from; NaN until the first sample.
    drive: f32,
}

impl Default for Overdrive {
    fn default() -> Self {
        Self {
            pre_gain: Ramp::default(),
            post_gain: Ramp::default(),
            gains: (0.0, 0.0),
            drive: f32::NAN,
        }
    }
}

impl Overdrive {
    /// `x` driven by `drive`, 0–1.
    #[inline(always)]
    pub fn process(&mut self, drive: f32, x: f32) -> f32 {
        if drive.to_bits() != self.drive.to_bits() {
            self.drive = drive;
            let drive_2 = drive * drive;
            let pre_gain_a = drive * 0.5;
            let pre_gain_b = drive_2 * drive_2 * drive * 24.0;
            let pre_gain = pre_gain_a + (pre_gain_b - pre_gain_a) * drive_2;
            let drive_squashed = drive * (2.0 - drive);
            let post_gain = 1.0 / soft_clip(0.33 + drive_squashed * (pre_gain - 0.33));
            self.gains = (pre_gain, post_gain);
        }
        self.pre_gain.set(self.gains.0, Some(1.0));
        self.post_gain.set(self.gains.1, Some(1.0));
        let pre = self.pre_gain.next_value() * x;
        soft_clip(pre) * self.post_gain.next_value()
    }
}
