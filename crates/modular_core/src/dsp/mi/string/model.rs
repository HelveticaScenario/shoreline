//! `plaits::String`: a delay-line string with a damping low-pass in its loop,
//! made inharmonic either by dispersion (an allpass stretch plus pitch noise)
//! or by a curved bridge that shortens the string as it swings.

use crate::dsp::mi::core::{
    delay_line::DelayLine,
    dsp::per_sample,
    filter::{Svf, tan_fast},
    interpolate::linear,
    random::Random,
    tables::LUT_SVF_SHIFT,
    units::semitones_to_ratio,
};

/// Plaits' string and stretch line lengths at 48 kHz.
const STRING_LENGTH: f32 = 1024.0;
const STRETCH_LENGTH: f32 = 256.0;

/// Per-sample constants for a sample rate.
#[derive(Clone, Copy, Default)]
pub struct StringRates {
    /// Cycles per sample here to cycles per sample at Plaits' 48 kHz.
    pub to_48k: f32,
    /// √`to_48k`: the gain that keeps white noise's in-band level at any rate.
    pub noise_gain: f32,
    sample_rate: f32,
    dc_pole: f32,
}

impl StringRates {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            to_48k: sample_rate / 48000.0,
            noise_gain: (sample_rate / 48000.0).sqrt(),
            sample_rate,
            dc_pole: 1.0 - 20.0 / sample_rate,
        }
    }

    fn line_length(&self, length: f32) -> usize {
        (length * self.to_48k).ceil() as usize
    }
}

/// The string's controls, derived from its pitch, nonlinearity, brightness
/// and damping.
#[derive(Clone, Copy, Default)]
struct Controls {
    /// Below the line's reach the string plays its lowest note and
    /// upsamples by this ratio; 1 otherwise.
    src_ratio: f32,
    upsampling: bool,
    delay: f32,
    g: f32,
    q: f32,
    dispersion: bool,
    stretch_point: f32,
    stretch_correction: f32,
    noise_amount: f32,
    noise_filter: f32,
    bridge_curving: f32,
    ap_gain: f32,
}

#[derive(Default)]
pub struct StringModel {
    string: DelayLine,
    stretch: DelayLine,
    damping_filter: Svf,
    dc_x: f32,
    dc_y: f32,
    delay: f32,
    dispersion_noise: f32,
    curved_bridge: f32,
    src_phase: f32,
    out_sample: [f32; 2],
    controls: Controls,
    /// The pitch, nonlinearity, brightness and damping `controls` were
    /// computed for; `NaN` = never computed.
    key: [f32; 4],
}

impl StringModel {
    /// A string with lines sized for `rates`. Allocates.
    pub fn new(rates: &StringRates) -> Self {
        Self {
            string: DelayLine::new(rates.line_length(STRING_LENGTH)),
            stretch: DelayLine::new(rates.line_length(STRETCH_LENGTH)),
            delay: 100.0,
            key: [f32::NAN; 4],
            ..Self::default()
        }
    }

    fn tune(
        &mut self,
        f0: f32,
        non_linearity: f32,
        brightness: f32,
        damping: f32,
        rates: &StringRates,
    ) {
        let key = [f0, non_linearity, brightness, damping];
        if key.map(f32::to_bits) == self.key.map(f32::to_bits) {
            return;
        }
        self.key = key;
        let dispersion = non_linearity > 0.0;
        let amount = if dispersion {
            non_linearity
        } else {
            -non_linearity
        };
        let max_delay = self.string.len() as f32 - 4.0;

        let mut delay = 1.0 / f0;
        delay = if delay < 4.0 {
            4.0
        } else if delay > max_delay {
            max_delay
        } else {
            delay
        };
        let mut src_ratio = delay * f0;
        let upsampling = src_ratio < 0.9999;
        if !upsampling {
            src_ratio = 1.0;
        }

        let mut brightness = brightness;
        let mut damping_cutoff = (12.0 + damping * damping * 60.0 + brightness * 24.0).min(84.0);
        let mut damping_f = (f0 * rates.to_48k * semitones_to_ratio(damping_cutoff)).min(0.499);
        // Crossfade to infinite decay.
        if damping >= 0.95 {
            let to_infinite = 20.0 * (damping - 0.95);
            brightness += to_infinite * (1.0 - brightness);
            damping_f += to_infinite * (0.4999 - damping_f);
            damping_cutoff += to_infinite * (128.0 - damping_cutoff);
        }
        let damping_compensation = linear(&LUT_SVF_SHIFT, damping_cutoff, 1.0);

        let stretch_point = amount * (2.0 - amount) * 0.225;
        let stretch_correction = ((160.0 / rates.sample_rate) * delay).clamp(1.0, 2.1);
        let noise_amount_sqrt = if amount > 0.75 {
            4.0 * (amount - 0.75)
        } else {
            0.0
        };
        let noise_filter = 0.06 + 0.94 * brightness * brightness;
        self.controls = Controls {
            src_ratio,
            upsampling,
            delay: delay * damping_compensation,
            g: g_at_rate(tan_fast(damping_f), rates.to_48k, tan_fast(0.4999)),
            q: 0.5,
            dispersion,
            stretch_point,
            stretch_correction,
            noise_amount: noise_amount_sqrt * noise_amount_sqrt * 0.1,
            noise_filter: per_sample(noise_filter, rates.to_48k),
            bridge_curving: amount * amount * 0.01,
            ap_gain: -0.618 * amount / (0.15 + amount.abs()),
        };
        self.damping_filter
            .set_g_q(self.controls.g, self.controls.q);
    }

    /// One sample of the string, excited by `input`. `f0` is in cycles per
    /// sample (finite, above 0); `non_linearity` runs from −1 (curved bridge)
    /// through 0 to 1 (dispersion).
    #[inline(always)]
    #[expect(clippy::too_many_arguments, reason = "mirrors Plaits' String::Process")]
    pub fn process(
        &mut self,
        f0: f32,
        non_linearity: f32,
        brightness: f32,
        damping: f32,
        input: f32,
        random: &mut Random,
        rates: &StringRates,
    ) -> f32 {
        self.tune(f0, non_linearity, brightness, damping, rates);
        let c = self.controls;
        if !c.upsampling {
            self.src_phase = 1.0;
        }

        self.src_phase += c.src_ratio;
        if self.src_phase > 1.0 {
            self.src_phase -= 1.0;
            // Plaits' one-sample ParameterInterpolator.
            self.delay += (c.delay - self.delay) / 1.0;
            let mut delay = self.delay;
            if c.dispersion {
                let noise = random.float() - 0.5;
                self.dispersion_noise += c.noise_filter * (noise - self.dispersion_noise);
                delay *= 1.0 + self.dispersion_noise * c.noise_amount;
            } else {
                delay *= 1.0 - self.curved_bridge * c.bridge_curving;
            }

            let mut s = if c.dispersion {
                let ap_delay = delay * c.stretch_point;
                let main_delay =
                    delay - ap_delay * (0.408 - c.stretch_point * 0.308) * c.stretch_correction;
                if ap_delay >= 4.0 && main_delay >= 4.0 {
                    let s = self.string.read(main_delay);
                    self.stretch.allpass(s, ap_delay as usize, c.ap_gain)
                } else {
                    self.string.read_hermite(delay)
                }
            } else {
                self.string.read_hermite(delay)
            };
            if !c.dispersion {
                let value = s.abs() - 0.025;
                let sign = if s > 0.0 { 1.0 } else { -1.5 };
                self.curved_bridge = (value.abs() + value) * sign;
            }

            s += input;
            s = s.clamp(-20.0, 20.0);
            let old_x = self.dc_x;
            self.dc_x = s;
            self.dc_y = self.dc_y * rates.dc_pole + s - old_x;
            let s = self.damping_filter.low_pass(self.dc_y);
            self.string.write(s);
            self.out_sample = [s, self.out_sample[0]];
        }
        let [newest, previous] = self.out_sample;
        previous + (newest - previous) * self.src_phase
    }
}

/// A filter's `g` at this rate for a `g` Plaits computes at 48 kHz. Scaling
/// `g`, rather than the cutoff, keeps the 48 kHz filter's response below
/// 24 kHz, and in the string's loop its loss and delay per pass, which set
/// decay and tuning; a cutoff near Nyquist is nearly transparent at 48 kHz.
/// Capped at `max`, Plaits' highest, so a low rate cannot push the filter
/// unstable.
pub fn g_at_rate(g_48k: f32, to_48k: f32, max: f32) -> f32 {
    (g_48k / to_48k).min(max)
}
