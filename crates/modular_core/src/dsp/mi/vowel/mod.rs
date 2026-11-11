//! Plaits' vowel synthesis, one module per synthesizer: `$mi.formantVowel`
//! (its formant filter bank), `$mi.samVowel` (SAM) and `$mi.lpcVowel` (its
//! LPC voice). Each runs as Plaits' speech engine runs that synth alone,
//! through its limiter.

pub mod lpc;
pub mod naive;
pub mod sam;

use crate::{
    dsp::{
        mi::core::{
            filter::tan_dirty,
            limiter::{Limiter, LimiterRates},
            lpc::SYNTHESIS_RATE,
            phase::{limit_frequency, max_frequency},
        },
        utils::SchmittTrigger,
    },
    types::{GlideMark, SwapGlide},
};

use lpc::CORRECTED_SAMPLE_RATE;

/// Plaits' gain into its limiter and its excitation gain, at ±5 V full
/// scale; both outputs clip at ±5 V as Plaits does.
const PRE_GAIN: f32 = 0.7;
const OUTPUT_GAIN: f32 = -5.0;
const EXCITATION_GAIN: f32 = -0.8 * 5.0;

/// The `vowel` and `formantShift` default every module declares.
const KNOB_DEFAULT: f32 = 2.5;

/// Rate conversions for the engine's rate, stated against Plaits' 48 kHz.
#[derive(Default)]
pub struct Rates {
    sample_rate: f32,
    inv_sample_rate: f32,
    /// Cycles per sample times this is cycles per 48 kHz sample.
    to_48k: f32,
    from_48k: f32,
    at_48k: bool,
    /// The impulse train's highest frequency, in cycles per sample.
    max_frequency: f32,
    /// SAM's highest glottal frequency, 3 kHz, in cycles per sample.
    sam_max_frequency: f32,
    /// A click's or SAM consonant's 50 ms.
    click_samples: usize,
    /// The impulse train's smoothing per sample, and its pulse height, which
    /// keeps the harmonics' level at any rate.
    impulse_smoothing: f32,
    /// `tan(π f)` of the 800 Hz band-pass that colours the impulses.
    coloration: f32,
    /// Engine samples per LPC synthesis tick at a formant ratio of 1.
    lpc_divider: f32,
    /// Cycles per sample times this is cycles per sample of Plaits'
    /// corrected rate, the unit its LPC pitch is reckoned in.
    lpc_pitch_scale: f32,
    /// An LPC frame's 25 ms.
    frame_samples: f32,
    limiter: LimiterRates,
}

impl Rates {
    fn new(sample_rate: f32) -> Self {
        let from_48k = 48000.0 / f64::from(sample_rate);
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            from_48k: from_48k as f32,
            at_48k: sample_rate == 48000.0,
            max_frequency: max_frequency(sample_rate).min(0.5),
            sam_max_frequency: 3000.0 / sample_rate,
            click_samples: (sample_rate * 0.05) as usize,
            impulse_smoothing: (1.0 - 0.75f64.powf(from_48k)) as f32,
            coloration: tan_dirty(800.0 / sample_rate),
            lpc_divider: sample_rate / SYNTHESIS_RATE,
            lpc_pitch_scale: (f64::from(sample_rate) / f64::from(CORRECTED_SAMPLE_RATE)) as f32,
            frame_samples: sample_rate / 40.0,
            limiter: LimiterRates::new(sample_rate),
        }
    }
}

/// One of Plaits' speech synths.
pub trait Voice: Default {
    fn new(_rates: &Rates) -> Self {
        Self::default()
    }

    /// One sample at `f0` cycles per sample: the excitation and the voice.
    /// `vowel` and `formant_shift` are 0–1; `trigger` starts a syllable.
    fn step(
        &mut self,
        trigger: bool,
        f0: f32,
        vowel: f32,
        formant_shift: f32,
        rates: &Rates,
    ) -> (f32, f32);
}

/// The knobs for a sample, in volts, with their glide marks.
pub struct Knobs {
    pub vowel: (f32, GlideMark),
    pub formant_shift: (f32, GlideMark),
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

/// One channel of a vowel module: its synth, then Plaits' limiter.
pub struct Channel<V> {
    voice: V,
    limiter: Limiter,
    trigger: SchmittTrigger,
    vowel: SwapGlide,
    formant_shift: SwapGlide,
    /// The last finite vowel and formant shift, which a NaN or infinite knob
    /// holds.
    held: [f32; 2],
}

impl<V: Voice> Default for Channel<V> {
    fn default() -> Self {
        Self {
            voice: V::default(),
            limiter: Limiter::default(),
            trigger: SchmittTrigger::default(),
            vowel: SwapGlide::default(),
            formant_shift: SwapGlide::default(),
            held: [KNOB_DEFAULT; 2],
        }
    }
}

impl<V: Voice> Channel<V> {
    fn new(rates: &Rates) -> Self {
        Self {
            voice: V::new(rates),
            ..Self::default()
        }
    }

    /// One sample at `f0` cycles per sample: the limited voice and the
    /// excitation, before the output gains. A rising `trigger` starts a
    /// syllable; `None` is an unpatched trigger.
    #[inline(always)]
    fn step(&mut self, f0: f32, knobs: Knobs, trigger: Option<f32>, rates: &Rates) -> (f32, f32) {
        let trigger = match trigger {
            Some(volts) => self.trigger.process(volts),
            None => {
                // A trigger patched again starts from low.
                self.trigger = SchmittTrigger::default();
                false
            }
        };
        let sample_rate = rates.sample_rate;
        let raw = [
            self.vowel.follow(knobs.vowel.0, knobs.vowel.1, sample_rate),
            self.formant_shift
                .follow(knobs.formant_shift.0, knobs.formant_shift.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let [vowel, formant_shift] = self.held.map(knob);
        let f0 = limit_frequency(f0, 0.5);
        let (excitation, out) = self.voice.step(trigger, f0, vowel, formant_shift, rates);
        (
            self.limiter.process(out * PRE_GAIN, &rates.limiter),
            excitation,
        )
    }

    /// One sample through the output stage: the voice and the excitation, in
    /// volts.
    #[inline(always)]
    fn output(&mut self, f0: f32, knobs: Knobs, trigger: Option<f32>, rates: &Rates) -> (f32, f32) {
        let (out, excitation) = self.step(f0, knobs, trigger, rates);
        (
            (OUTPUT_GAIN * out).clamp(-5.0, 5.0),
            (EXCITATION_GAIN * excitation).clamp(-5.0, 5.0),
        )
    }
}

/// The body of a vowel module's `update`: every channel of `$module`, whose
/// params are `$params`, one sample.
macro_rules! update_channels {
    ($module:expr, $params:ident) => {{
        let module = $module;
        let channels = module.channel_count();
        let p = &module.params;
        let rates = &module.state;
        for (ch, channel) in module.channel_state[..channels].iter_mut().enumerate() {
            let f0 = crate::dsp::oscillators::apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, $params::FM_DEFAULT),
                p.fm_mode,
            ) * rates.inv_sample_rate;
            let trigger = p.trigger.as_ref().map(|t| t.get_value(ch));
            let knobs = super::Knobs {
                vowel: (
                    p.vowel.value_or(ch, $params::VOWEL_DEFAULT),
                    p.vowel.glide_mark(ch),
                ),
                formant_shift: (
                    p.formant_shift.value_or(ch, $params::FORMANT_SHIFT_DEFAULT),
                    p.formant_shift.glide_mark(ch),
                ),
            };
            let (out, excitation) = channel.output(f0, knobs, trigger, rates);
            module.outputs.sample.set(ch, out);
            module.outputs.excitation.set(ch, excitation);
        }
    }};
}
use update_channels;

#[cfg(test)]
mod tests;
