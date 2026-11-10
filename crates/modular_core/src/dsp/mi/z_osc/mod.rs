mod z;

use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{
            filter::{OnePole, tan_dirty},
            phase::{limit_frequency, max_frequency},
        },
        oscillators::{FmMode, apply_fm},
        utils::changed,
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use z::{ZControls, ZOscillator};

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.6 * 5.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ZOscParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "resonance, from the pitch itself (0) up to eight octaves above it (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    cutoff: Option<PolySignal>,
    #[input(
        "carrier contour, from an unshaped pulse train (0) through a smooth window (2.5) to phase-shifted sines (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    shape: Option<PolySignal>,
    #[input(
        "response, through low-pass, peaking, band-pass and high-pass characters in turn",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    mode: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs the waveform backward",
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
struct ZOscOutputs {
    #[output("output", "the filtered-sounding waveform", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

const Z_OSC_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonant-filter-like oscillator",
    description: "The Z oscillator from Mutable Instruments Plaits' granular \
                  formant engine. A sine restarts every half cycle under a \
                  decaying ramp, so it sounds like a waveform through a resonant \
                  filter whose resonance tracks the pitch, with no filter at all.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[DocExample {
        title: "Resonant sweep",
        code: r#"
            const sweep = $sine('0.2hz').range(1, 4)
            $mi.zOsc('c2', { cutoff: sweep, mode: 1 }).out()
        "#,
    }],
};

#[module(name = "$mi.zOsc", docs = Z_OSC_DOCS, args(freq), has_init, block)]
pub struct ZOsc {
    outputs: ZOscOutputs,
    state: ZOscState,
    channel_state: Box<[Voice]>,
    params: ZOscParams,
}

#[derive(Default)]
struct ZOscState {
    inv_sample_rate: f32,
    max_carrier: f32,
    max_formant: f32,
}

impl ZOscState {
    fn new(sample_rate: f32) -> Self {
        Self {
            inv_sample_rate: 1.0 / sample_rate,
            max_carrier: 0.5 * max_frequency(sample_rate),
            max_formant: max_frequency(sample_rate),
        }
    }
}

#[derive(Clone, Copy)]
struct Voice {
    osc: ZOscillator,
    dc_blocker: OnePole,
    /// Set by the first `control`, which jumps the controls to their targets.
    started: bool,
    cutoff: SwapGlide,
    shape: SwapGlide,
    mode: SwapGlide,
    /// Formant-to-pitch ratio for the cached `cutoff`; `NaN` = never computed.
    last_cutoff: f32,
    multiplier: f32,
    /// Pitch the DC blocker is tuned to; `NaN` = never tuned.
    last_f0: f32,
    /// The raw pitch and knobs of the last `control`; `NaN` = never set.
    last_inputs: [f32; 4],
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            osc: ZOscillator::default(),
            dc_blocker: OnePole::default(),
            started: false,
            cutoff: SwapGlide::default(),
            shape: SwapGlide::default(),
            mode: SwapGlide::default(),
            last_cutoff: f32::NAN,
            multiplier: 0.0,
            last_f0: f32::NAN,
            last_inputs: [f32::NAN; 4],
        }
    }
}

/// The control inputs for a sample: the raw signals and their glide marks.
struct Knobs {
    cutoff: (f32, GlideMark),
    shape: (f32, GlideMark),
    mode: (f32, GlideMark),
}

impl Voice {
    /// Sets the controls the oscillator ramps to over the next `length`
    /// samples, recomputing the costly ones only when their inputs move.
    #[inline(always)]
    fn control(&mut self, f0: f32, knobs: Knobs, length: f32, sample_rate: f32, state: &ZOscState) {
        // Unchanged inputs on settled ramps would leave every control as it is.
        let inputs = [f0, knobs.cutoff.0, knobs.shape.0, knobs.mode.0];
        if inputs == self.last_inputs
            && self.osc.settled()
            && !self.cutoff.will_glide(knobs.cutoff.1)
            && !self.shape.will_glide(knobs.shape.1)
            && !self.mode.will_glide(knobs.mode.1)
        {
            return;
        }
        self.last_inputs = inputs;
        // Bounded at Nyquist, so an extreme pitch cannot destabilise the DC
        // blocker.
        let f0 = limit_frequency(f0, 0.5);
        let cutoff = self
            .cutoff
            .follow(knobs.cutoff.0, knobs.cutoff.1, sample_rate);
        let shape = self.shape.follow(knobs.shape.0, knobs.shape.1, sample_rate);
        let mode = self.mode.follow(knobs.mode.0, knobs.mode.1, sample_rate);
        // A NaN or infinite cutoff holds the multiplier where it is.
        if changed(cutoff, self.last_cutoff) && cutoff.is_finite() {
            self.multiplier = (8.0 * (cutoff * 0.2).clamp(0.0, 1.0)).exp2();
            self.last_cutoff = cutoff;
        }
        let controls = ZControls {
            formant: (f0.abs() * self.multiplier).min(state.max_formant),
            shape: (shape * 0.2).clamp(0.0, 1.0),
            mode: (mode * 0.2).clamp(0.0, 1.0),
        };
        self.osc.set(&controls, self.started.then_some(length));
        self.started = true;
        if changed(f0, self.last_f0) {
            self.dc_blocker.set_g(tan_dirty(0.3 * f0.abs()));
            self.last_f0 = f0;
        }
    }

    #[inline(always)]
    fn step(&mut self, f0: f32, state: &ZOscState) -> f32 {
        let f0 = limit_frequency(f0, state.max_carrier);
        OUTPUT_GAIN * self.dc_blocker.high_pass(self.osc.step(f0))
    }
}

impl ZOsc {
    fn init(&mut self, sample_rate: f32) {
        self.state = ZOscState::new(sample_rate);
    }

    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        for (ch, voice) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, ZOscParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                cutoff: (
                    p.cutoff.value_or(ch, ZOscParams::CUTOFF_DEFAULT),
                    p.cutoff.glide_mark(ch),
                ),
                shape: (
                    p.shape.value_or(ch, ZOscParams::SHAPE_DEFAULT),
                    p.shape.glide_mark(ch),
                ),
                mode: (
                    p.mode.value_or(ch, ZOscParams::MODE_DEFAULT),
                    p.mode.glide_mark(ch),
                ),
            };
            voice.control(f0, knobs, 1.0, sample_rate, &self.state);
            self.outputs.sample.set(ch, voice.step(f0, &self.state));
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut ZOscBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let inputs = p.block_inputs(range.clone(), channels, &mut scratch)?;
        let state = &self.state;
        for (ch, voice) in self.channel_state[..channels].iter_mut().enumerate() {
            let marks = (
                p.cutoff.glide_mark(ch),
                p.shape.glide_mark(ch),
                p.mode.glide_mark(ch),
            );
            for i in range.clone() {
                let f0 = apply_fm(inputs.freq.row(i)[ch], inputs.fm.row(i)[ch], p.fm_mode)
                    * state.inv_sample_rate;
                let knobs = Knobs {
                    cutoff: (inputs.cutoff.row(i)[ch], marks.0),
                    shape: (inputs.shape.row(i)[ch], marks.1),
                    mode: (inputs.mode.row(i)[ch], marks.2),
                };
                voice.control(f0, knobs, 1.0, sample_rate, state);
                outputs.sample.slot_mut(i)[ch] = voice.step(f0, state);
            }
        }
        Some(())
    }
}

message_handlers!(impl ZOsc {});

#[cfg(test)]
mod tests;
