mod grainlet;

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
        utils::{changed, voct_to_hz},
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use grainlet::{GrainletControls, GrainletPair};

/// Plaits' output gain for this engine, at ±5 V full scale.
const OUTPUT_GAIN: f32 = -0.7 * 5.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct FormantParams {
    #[input("pitch: how often the grains repeat", type = pitch)]
    freq: PolySignal,
    #[input(
        "first formant, independent of the pitch",
        type = frequency,
        default = 0.5,
    )]
    #[deserr(default)]
    formant: Option<PolySignal>,
    #[input(
        "second formant, from two octaves below the first (0) to two above (5); below 2.5 the bare window bleeds in",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    ratio: Option<PolySignal>,
    #[input(
        "window shape, from a narrow pulse (0) to broad and flat-topped (5); narrows by itself at high pitches",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    width: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero plays each grain's window backward",
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
struct FormantOutputs {
    #[output("output", "the paired grains", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

const FORMANT_DOCS: ModuleDocs = ModuleDocs {
    summary: "Granular formant oscillator",
    description: "Mutable Instruments Plaits' granular formant oscillator. Every \
                  cycle starts a pair of short sine grains, giving vowel-like, \
                  resonant timbres whose formants stay put as the pitch moves.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[DocExample {
        title: "Sweeping vowel bass",
        code: r#"
            const vowel = $sine('0.1hz').range(0, 2)
            $mi.formant('c2', { formant: vowel }).out()
        "#,
    }],
};

#[module(name = "$mi.formant", docs = FORMANT_DOCS, args(freq), has_init, block)]
pub struct Formant {
    outputs: FormantOutputs,
    state: FormantState,
    lane_state: Box<[Voice]>,
    params: FormantParams,
}

#[derive(Default)]
struct FormantState {
    inv_sample_rate: f32,
    /// Reciprocal of the pitch, in cycles per sample, at which `width` has
    /// fully narrowed.
    narrowing: f32,
    max_carrier: f32,
    max_formant: f32,
}

impl FormantState {
    fn new(sample_rate: f32) -> Self {
        Self {
            inv_sample_rate: 1.0 / sample_rate,
            narrowing: sample_rate / 2000.0,
            max_carrier: 0.5 * max_frequency(sample_rate),
            max_formant: max_frequency(sample_rate),
        }
    }
}

#[derive(Clone, Copy)]
struct Voice {
    osc: GrainletPair,
    dc_blocker: OnePole,
    /// Set by the first `control`, which jumps the controls to their targets.
    started: bool,
    formant: SwapGlide,
    ratio: SwapGlide,
    width: SwapGlide,
    /// Formant frequencies and bleed for the cached `formant`/`ratio` pair;
    /// `NaN` = never computed.
    last_formant: f32,
    last_ratio: f32,
    formants: [f32; 2],
    bleed: f32,
    /// Pitch the DC blocker is tuned to; `NaN` = never tuned.
    last_f0: f32,
    /// The raw pitch and knobs of the last `control`; `NaN` = never set.
    last_inputs: [f32; 4],
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            osc: GrainletPair::default(),
            dc_blocker: OnePole::default(),
            started: false,
            formant: SwapGlide::default(),
            ratio: SwapGlide::default(),
            width: SwapGlide::default(),
            last_formant: f32::NAN,
            last_ratio: f32::NAN,
            formants: [0.0; 2],
            bleed: 0.0,
            last_f0: f32::NAN,
            last_inputs: [f32::NAN; 4],
        }
    }
}

/// The control inputs for a sample: the raw signals and their glide marks.
struct Knobs {
    formant: (f32, GlideMark),
    ratio: (f32, GlideMark),
    width: (f32, GlideMark),
}

impl Voice {
    /// Sets the controls the oscillator ramps to over the next `length`
    /// samples, recomputing the costly ones only when their inputs move.
    #[inline(always)]
    fn control(
        &mut self,
        f0: f32,
        knobs: Knobs,
        length: f32,
        sample_rate: f32,
        state: &FormantState,
    ) {
        // Unchanged inputs on settled ramps would leave every control as it is.
        let inputs = [f0, knobs.formant.0, knobs.ratio.0, knobs.width.0];
        if inputs == self.last_inputs
            && self.osc.settled()
            && !self.formant.will_glide(knobs.formant.1)
            && !self.ratio.will_glide(knobs.ratio.1)
            && !self.width.will_glide(knobs.width.1)
        {
            return;
        }
        self.last_inputs = inputs;
        // Bounded at Nyquist, so an extreme pitch cannot destabilise the DC
        // blocker.
        let f0 = limit_frequency(f0, 0.5);
        let formant = self
            .formant
            .follow(knobs.formant.0, knobs.formant.1, sample_rate);
        let ratio = self.ratio.follow(knobs.ratio.0, knobs.ratio.1, sample_rate);
        let width = self.width.follow(knobs.width.0, knobs.width.1, sample_rate);

        // A NaN or infinite knob holds the formants where they are.
        if (changed(formant, self.last_formant) || changed(ratio, self.last_ratio))
            && formant.is_finite()
            && ratio.is_finite()
        {
            let f1 = voct_to_hz(formant) * state.inv_sample_rate;
            let harmonics = (ratio * 0.2).clamp(0.0, 1.0);
            let multiplier = (4.0 * harmonics - 2.0).exp2();
            let bleed = if harmonics < 0.5 {
                1.0 - 2.0 * harmonics
            } else {
                0.0
            };
            self.formants = [
                f1.min(state.max_formant),
                (f1 * multiplier).min(state.max_formant),
            ];
            self.bleed = bleed * (2.0 - bleed);
            self.last_formant = formant;
            self.last_ratio = ratio;
        }
        let morph = (width * 0.2).clamp(0.0, 1.0);
        let controls = GrainletControls {
            formant: self.formants,
            shape: 0.33 + (morph - 0.33) * (1.0 - f0.abs() * state.narrowing).max(0.0),
            bleed: self.bleed,
        };
        self.osc.set(&controls, self.started.then_some(length));
        self.started = true;
        if changed(f0, self.last_f0) {
            self.dc_blocker.set_g(tan_dirty(0.3 * f0.abs()));
            self.last_f0 = f0;
        }
    }

    #[inline(always)]
    fn step(&mut self, f0: f32, state: &FormantState) -> f32 {
        let f0 = limit_frequency(f0, state.max_carrier);
        OUTPUT_GAIN * self.dc_blocker.high_pass(self.osc.step(f0))
    }
}

impl Formant {
    fn init(&mut self, sample_rate: f32) {
        self.state = FormantState::new(sample_rate);
    }

    fn update(&mut self, sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        for (lane, voice) in self.lane_state[..lanes].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(lane),
                p.fm.value_or(lane, FormantParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let knobs = Knobs {
                formant: (
                    p.formant.value_or(lane, FormantParams::FORMANT_DEFAULT),
                    p.formant.glide_mark(lane),
                ),
                ratio: (
                    p.ratio.value_or(lane, FormantParams::RATIO_DEFAULT),
                    p.ratio.glide_mark(lane),
                ),
                width: (
                    p.width.value_or(lane, FormantParams::WIDTH_DEFAULT),
                    p.width.glide_mark(lane),
                ),
            };
            voice.control(f0, knobs, 1.0, sample_rate, &self.state);
            self.outputs.sample.set(lane, voice.step(f0, &self.state));
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut FormantBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let inputs = p.block_inputs(range.clone(), lanes, &mut scratch)?;
        let state = &self.state;
        for (lane, voice) in self.lane_state[..lanes].iter_mut().enumerate() {
            let marks = (
                p.formant.glide_mark(lane),
                p.ratio.glide_mark(lane),
                p.width.glide_mark(lane),
            );
            for i in range.clone() {
                let f0 = apply_fm(inputs.freq.row(i)[lane], inputs.fm.row(i)[lane], p.fm_mode)
                    * state.inv_sample_rate;
                let knobs = Knobs {
                    formant: (inputs.formant.row(i)[lane], marks.0),
                    ratio: (inputs.ratio.row(i)[lane], marks.1),
                    width: (inputs.width.row(i)[lane], marks.2),
                };
                voice.control(f0, knobs, 1.0, sample_rate, state);
                outputs.sample.slot_mut(i)[lane] = voice.step(f0, state);
            }
        }
        Some(())
    }
}

message_handlers!(impl Formant {});

#[cfg(test)]
mod tests;
