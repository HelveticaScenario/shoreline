mod spectrum;

use deserr::Deserr;
use schemars::JsonSchema;
use serde::Serialize;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{phase::limit_frequency, sine::sine},
        oscillators::{FmMode, apply_fm},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use spectrum::{BANK, INTEGER, INTEGER_SLOTS, ORGAN, ORGAN_SLOTS, SLOTS, bank, update_amplitudes};

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.8 * 5.0;

/// How often the spectrum steps toward the knobs: Plaits' block rate.
const SPECTRUM_RATE: f32 = 4000.0;

/// Which harmonics sound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, Serialize, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum Series {
    /// every harmonic from the fundamental to the 24th
    #[default]
    Integer,
    /// harmonics 1, 2, 3, 4, 6, 8, 10 and 12, a drawbar organ's footages
    Organ,
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct AdditiveParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "the loudest harmonic, from the fundamental (0) to the top of the series (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    centroid: Option<PolySignal>,
    #[input(
        "how fast the harmonics fade either side of the centroid: a broad, bright spread (0) to the centroid's harmonic alone (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    slope: Option<PolySignal>,
    #[input(
        "ripples across the spectrum, more and closer as it rises; also broadens it",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    bumps: Option<PolySignal>,
    #[input("which harmonics sound")]
    #[serde(default)]
    #[deserr(default)]
    series: Series,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs the wave backward",
        type = audio,
    )]
    #[deserr(default)]
    fm: Option<PolySignal>,
    #[input("how `fm` bends the frequency")]
    #[serde(default)]
    #[deserr(default)]
    fm_mode: FmMode,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AdditiveOutputs {
    #[output("output", "the summed harmonics", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

const ADDITIVE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Additive oscillator",
    description: "Mutable Instruments Plaits' additive oscillator: up to 24 \
                  sine harmonics whose levels form a peak you can sweep, \
                  narrow and ripple, from hollow organ tones to glassy, \
                  vocal sweeps. The spectrum glides to new settings over \
                  about a twentieth of a second.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Sweeping harmonics",
            code: r#"
                const sweep = $sine('0.1hz').range(0, 5)
                $mi.additive('c2', { centroid: sweep, slope: 4 }).out()
            "#,
        },
        DocExample {
            title: "Drawbar organ",
            code: "$mi.additive('c3', { centroid: 1, series: 'organ' }).out()",
        },
    ],
};

#[module(name = "$mi.additive", docs = ADDITIVE_DOCS, args(freq), has_init)]
pub struct Additive {
    outputs: AdditiveOutputs,
    state: AdditiveState,
    channel_state: Box<[Voice]>,
    params: AdditiveParams,
}

#[derive(Default)]
struct AdditiveState {
    sample_rate: f32,
    inv_sample_rate: f32,
    to_48k: f32,
    /// Samples per spectrum step: 12 at 48 kHz.
    period: f32,
}

impl AdditiveState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            to_48k: sample_rate / 48000.0,
            period: sample_rate / SPECTRUM_RATE,
        }
    }
}

#[derive(Clone, Copy)]
struct Voice {
    phase: f32,
    /// Plaits' smoothed, normalised amplitudes, one step per `period`.
    amplitudes: [f32; SLOTS],
    /// The amplitudes ramped across each step, and their per-sample steps.
    ramp: [f32; SLOTS],
    increment: [f32; SLOTS],
    /// Each integer harmonic's anti-aliasing gain for `last_f`; the organ
    /// bank reads the first 12. `NaN` = never computed.
    taper: [f32; INTEGER],
    last_f: f32,
    /// Samples until the next spectrum step.
    countdown: f32,
    /// The knobs summed since the last step, and how many samples that is.
    sum: [f32; 3],
    count: f32,
    centroid: SwapGlide,
    slope: SwapGlide,
    bumps: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 3],
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            phase: 0.0,
            amplitudes: [0.0; SLOTS],
            ramp: [0.0; SLOTS],
            increment: [0.0; SLOTS],
            taper: [0.0; INTEGER],
            last_f: f32::NAN,
            countdown: 0.0,
            sum: [0.0; 3],
            count: 0.0,
            centroid: SwapGlide::default(),
            slope: SwapGlide::default(),
            bumps: SwapGlide::default(),
            held: [
                AdditiveParams::CENTROID_DEFAULT,
                AdditiveParams::SLOPE_DEFAULT,
                AdditiveParams::BUMPS_DEFAULT,
            ],
        }
    }
}

/// The control inputs for a sample: the raw signals and their glide marks.
struct Knobs {
    centroid: (f32, GlideMark),
    slope: (f32, GlideMark),
    bumps: (f32, GlideMark),
}

fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Voice {
    /// Reads a sample's knobs and, when a step is due, moves the spectrum
    /// toward their mean since the last step and ramps to it.
    #[inline(always)]
    fn control(&mut self, knobs: Knobs, state: &AdditiveState) {
        let sample_rate = state.sample_rate;
        let raw = [
            self.centroid
                .follow(knobs.centroid.0, knobs.centroid.1, sample_rate),
            self.slope.follow(knobs.slope.0, knobs.slope.1, sample_rate),
            self.bumps.follow(knobs.bumps.0, knobs.bumps.1, sample_rate),
        ];
        for ((held, sum), value) in self.held.iter_mut().zip(&mut self.sum).zip(raw) {
            if value.is_finite() {
                *held = value;
            }
            *sum += *held;
        }
        self.count += 1.0;
        if self.countdown <= 0.0 {
            self.countdown += state.period;
            let [centroid, morph, raw_bumps] = self.sum.map(|sum| knob(sum / self.count));
            self.sum = [0.0; 3];
            self.count = 0.0;
            let raw_slope = (1.0 - 0.6 * raw_bumps) * morph;
            let slope = 0.01 + 1.99 * raw_slope * raw_slope * raw_slope;
            let bumps = 16.0 * raw_bumps * raw_bumps;
            update_amplitudes(centroid, slope, bumps, &mut self.amplitudes, &INTEGER_SLOTS);
            update_amplitudes(
                centroid,
                slope,
                bumps,
                &mut self.amplitudes[ORGAN..],
                &ORGAN_SLOTS,
            );
            for ((increment, target), value) in self
                .increment
                .iter_mut()
                .zip(&self.amplitudes)
                .zip(&self.ramp)
            {
                *increment = (target - value) / state.period;
            }
        }
        self.countdown -= 1.0;
        for (value, increment) in self.ramp.iter_mut().zip(&self.increment) {
            *value += increment;
        }
    }

    /// One sample at `f0` cycles per sample, before the output gain.
    #[inline(always)]
    fn step(&mut self, f0: f32, series: Series, state: &AdditiveState) -> f32 {
        let f = limit_frequency(f0, 0.5);
        let pitch = f.abs();
        if pitch.to_bits() != self.last_f.to_bits() {
            // Harmonics fade toward Plaits' 24 kHz, and to silence at this
            // rate's Nyquist when that is lower.
            let fundamental = pitch.max(pitch * state.to_48k);
            for (k, taper) in self.taper.iter_mut().enumerate() {
                let harmonic = (fundamental * (k + 1) as f32).min(0.5);
                *taper = 1.0 - harmonic * 2.0;
            }
            self.last_f = pitch;
        }
        self.phase += f;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        } else if self.phase < 0.0 {
            self.phase += 1.0;
        }
        let two_x = 2.0 * sine(self.phase);
        let mut amplitudes = [0.0; INTEGER];
        match series {
            Series::Integer => {
                for (a, (ramp, taper)) in
                    amplitudes.iter_mut().zip(self.ramp.iter().zip(&self.taper))
                {
                    *a = ramp * taper;
                }
                bank::<1>(self.phase, two_x, &amplitudes[..BANK])
                    + bank::<13>(self.phase, two_x, &amplitudes[BANK..])
            }
            Series::Organ => {
                for (a, (ramp, taper)) in amplitudes
                    .iter_mut()
                    .zip(self.ramp[ORGAN..].iter().zip(&self.taper))
                {
                    *a = ramp * taper;
                }
                bank::<1>(self.phase, two_x, &amplitudes[..BANK])
            }
        }
    }
}

impl Additive {
    fn init(&mut self, sample_rate: f32) {
        self.state = AdditiveState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        for (ch, voice) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, AdditiveParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                centroid: (
                    p.centroid.value_or(ch, AdditiveParams::CENTROID_DEFAULT),
                    p.centroid.glide_mark(ch),
                ),
                slope: (
                    p.slope.value_or(ch, AdditiveParams::SLOPE_DEFAULT),
                    p.slope.glide_mark(ch),
                ),
                bumps: (
                    p.bumps.value_or(ch, AdditiveParams::BUMPS_DEFAULT),
                    p.bumps.glide_mark(ch),
                ),
            };
            voice.control(knobs, &self.state);
            let out = voice.step(f0, p.series, &self.state);
            self.outputs.sample.set(ch, OUTPUT_GAIN * out);
        }
    }
}

message_handlers!(impl Additive {});

#[cfg(test)]
mod tests;
