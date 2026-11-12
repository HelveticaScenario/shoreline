//! Full-featured Plaits voice module based on Mutable Instruments Plaits.
//! Adapted from VCV Rack implementation with additional features.
//!
//! This module wraps the complete mi-plaits-dsp Voice struct, providing:
//! - All 24 synthesis engines with engine selection
//! - Built-in Low Pass Gate (LPG) with color and decay parameters
//! - Trigger delay (1ms) for sequencer/MIDI interface timing correction
//! - Modulation attenuverters for FM, timbre, and morph
//! - Level input for dynamics/VCA control
//! - Proper modulation routing matching the original hardware

use deserr::Deserr;
use mi_plaits_dsp::voice::{Modulations, Patch, Voice};
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, DocSection, DocTable, ModuleDocs};
use crate::{
    dsp::utils::{SchmittTrigger, voct_to_midi},
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};

/// Block size for rendering - matches VCV Rack's implementation
const BLOCK_SIZE: usize = 12;

/// Plaits' knob filters as time constants. The firmware runs
/// `ONE_POLE(value, adc, c)` once per 12-sample block at 48 kHz, with c = 0.01
/// for timbre and morph (and their hidden LPG colour and decay) and c = 0.005
/// for harmonics and the attenuverters: τ = -(12 / 48000) / ln(1 - c). Patch
/// edits glide like a knob turn; live signals arrive unfiltered, like its CV
/// jacks.
const KNOB_TAU_FAST: f32 = 0.024_875;
const KNOB_TAU_SLOW: f32 = 0.049_875;

/// Internal sample rate for the Plaits engine (designed for 48kHz)
const ENGINE_SAMPLE_RATE: f32 = 48000.0;

/// Synthesis engine selection for Plaits
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum PlaitsEngine {
    /// one analog-style oscillator through a resonant filter, for classic subtractive sounds
    #[default]
    VaVcf,
    /// phase distortion, the Casio CZ sound of sweeping resonant tones
    PhaseDistortion,
    /// DX7-style six-operator FM, preset bank A
    SixOpA,
    /// DX7-style six-operator FM, preset bank B
    SixOpB,
    /// DX7-style six-operator FM, preset bank C
    SixOpC,
    /// wave terrain, shifting and evolving digital tones
    WaveTerrain,
    /// string machine, lush ensemble chords
    StringMachine,
    /// chiptune square waves with a built-in arpeggiator
    Chiptune,
    /// two raw analog-style oscillators with no filter, for detuned saws and pulses
    VirtualAnalog,
    /// a folded, waveshaped oscillator, bright and buzzy
    Waveshaping,
    /// two-operator FM, from bell tones to metallic growls
    TwoOpFm,
    /// formant oscillator, vocal and nasal tones
    GranularFormant,
    /// additive harmonics, organ-like tones
    Additive,
    /// wavetable scanning, classic digital sweeps
    Wavetable,
    /// stacked chords from a single pitch
    Chords,
    /// vowels and robotic speech
    Speech,
    /// a swarm of detuned oscillators, huge and smeary
    Swarm,
    /// noise through a resonant filter, from hiss to whistles
    FilteredNoise,
    /// random dust and droplets ringing through filters
    ParticleNoise,
    /// a plucked string, for guitar and sitar tones
    InharmonicString,
    /// a struck resonator for bells, bars and wood
    ModalResonator,
    /// analog-style kick drum
    BassDrum,
    /// analog-style snare drum
    SnareDrum,
    /// analog-style hi-hat
    HiHat,
}

impl PlaitsEngine {
    /// Convert engine enum to index used by mi-plaits-dsp
    fn to_index(self) -> usize {
        match self {
            PlaitsEngine::VaVcf => 0,
            PlaitsEngine::PhaseDistortion => 1,
            PlaitsEngine::SixOpA => 2,
            PlaitsEngine::SixOpB => 3,
            PlaitsEngine::SixOpC => 4,
            PlaitsEngine::WaveTerrain => 5,
            PlaitsEngine::StringMachine => 6,
            PlaitsEngine::Chiptune => 7,
            PlaitsEngine::VirtualAnalog => 8,
            PlaitsEngine::Waveshaping => 9,
            PlaitsEngine::TwoOpFm => 10,
            PlaitsEngine::GranularFormant => 11,
            PlaitsEngine::Additive => 12,
            PlaitsEngine::Wavetable => 13,
            PlaitsEngine::Chords => 14,
            PlaitsEngine::Speech => 15,
            PlaitsEngine::Swarm => 16,
            PlaitsEngine::FilteredNoise => 17,
            PlaitsEngine::ParticleNoise => 18,
            PlaitsEngine::InharmonicString => 19,
            PlaitsEngine::ModalResonator => 20,
            PlaitsEngine::BassDrum => 21,
            PlaitsEngine::SnareDrum => 22,
            PlaitsEngine::HiHat => 23,
        }
    }
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
struct PlaitsParams {
    #[input("note pitch", type = pitch)]
    freq: PolySignal,

    #[input("synthesis engine")]
    engine: PlaitsEngine,

    #[input(
        "harmonics knob; what it does depends on the engine",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    harmonics: Option<PolySignal>,

    #[input(
        "timbre knob; what it does depends on the engine",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    timbre: Option<PolySignal>,

    #[input(
        "morph knob; what it does depends on the engine",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    morph: Option<PolySignal>,

    #[input("frequency modulation, scaled by `fmAmt`", type = audio)]
    #[deserr(default)]
    fm: Option<PolySignal>,

    #[input(
        "how much the trigger's internal envelope sweeps timbre; negative inverts",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    timbre_amt: Option<PolySignal>,

    #[input(
        "how much `fm` bends the pitch, negative inverting it. With `fm` unpatched it sets how far each trigger's envelope sweeps the pitch instead, or, with no trigger either, fine-tunes by up to a semitone",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    fm_amt: Option<PolySignal>,

    #[input(
        "how much the trigger's internal envelope sweeps morph; negative inverts",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    morph_amt: Option<PolySignal>,

    #[input(
        "strikes the internal envelope and lowpass gate; unpatched, the voice drones",
        type = trig,
    )]
    #[deserr(default)]
    trigger: Option<PolySignal>,

    #[input(
        "loudness and brightness through the lowpass gate; patching it opens the gate by this level instead of the trigger's ping",
        type = control,
        range = (0.0, 8.0),
        control_range = (0.0, 5.0),
    )]
    #[deserr(default)]
    level: Option<PolySignal>,

    #[input(
        "lowpass gate response: low is a mellow filter sweep, high a bright VCA",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    lpg_color: Option<PolySignal>,

    #[input(
        "how long the lowpass gate and internal envelope ring after a trigger",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    lpg_decay: Option<PolySignal>,

    #[input(
        "blend at `blend` between the engine's `main` sound at 0 and its `aux` variant at 5",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    mix: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PlaitsOutputs {
    #[output("blend", "main and aux blended by `mix`", type = cv, default, range = (-5.0, 5.0))]
    mix: PolyOutput,

    #[output("main", "the engine's main output", type = cv, range = (-5.0, 5.0))]
    main: PolyOutput,

    #[output("aux", "the engine's auxiliary output, a variant that depends on the engine", type = cv, range = (-5.0, 5.0))]
    aux: PolyOutput,
}

/// Per-channel voice state
struct PlaitsChannelState {
    voice: Voice<'static>,
    main_buffer: [f32; BLOCK_SIZE],
    aux_buffer: [f32; BLOCK_SIZE],
    // Glided parameters
    harmonics: SwapGlide,
    timbre_amt: SwapGlide,
    fm_amt: SwapGlide,
    morph_amt: SwapGlide,
    timbre: SwapGlide,
    morph: SwapGlide,
    lpg_color: SwapGlide,
    lpg_decay: SwapGlide,
    mix: SwapGlide,
    // Trigger state tracking
    /// Schmitt trigger for edge detection with hysteresis
    trigger_schmitt: SchmittTrigger,
    /// Latched trigger value - captures any trigger that occurred since last block render
    /// This ensures short triggers (even 1 sample) aren't missed between block boundaries
    trigger_latch: bool,
}

impl Default for PlaitsChannelState {
    fn default() -> Self {
        let mut voice = Voice::new(BLOCK_SIZE, ENGINE_SAMPLE_RATE);
        voice.init();
        Self {
            voice,
            main_buffer: [0.0; BLOCK_SIZE],
            aux_buffer: [0.0; BLOCK_SIZE],
            harmonics: SwapGlide::with_time_constant(KNOB_TAU_SLOW),
            timbre_amt: SwapGlide::with_time_constant(KNOB_TAU_SLOW),
            fm_amt: SwapGlide::with_time_constant(KNOB_TAU_SLOW),
            morph_amt: SwapGlide::with_time_constant(KNOB_TAU_SLOW),
            timbre: SwapGlide::with_time_constant(KNOB_TAU_FAST),
            morph: SwapGlide::with_time_constant(KNOB_TAU_FAST),
            lpg_color: SwapGlide::with_time_constant(KNOB_TAU_FAST),
            lpg_decay: SwapGlide::with_time_constant(KNOB_TAU_FAST),
            mix: SwapGlide::default(),
            trigger_schmitt: SchmittTrigger::default(),
            trigger_latch: false,
        }
    }
}

const MACRO_DOCS: ModuleDocs = ModuleDocs {
    summary: "Macro oscillator (Plaits), deprecated",
    description: "**Deprecated:** each engine has its own module, listed under \
                  Replacements, and `$macro` will be removed. Mutable \
                  Instruments Plaits: 24 engines, each played with the same \
                  three knobs, `harmonics`, `timbre` and `morph`, which \
                  triggers strike through a built-in envelope and lowpass \
                  gate. The [Plaits \
                  manual](https://pichenettes.github.io/mutable-instruments-documentation/modules/plaits/) \
                  describes what the knobs do on each engine.",
    categories: &[ModuleCategory::Oscillator, ModuleCategory::Drum],
    sections: &[DocSection {
        title: "Replacements",
        body: "The replacements have no built-in lowpass gate or trigger \
               envelope: where `$macro` gated the sound, patch `$lpg` after \
               them, and send `$perc` or `$adsr` to any param for the sweeps \
               `timbreAmt`, `morphAmt` and `fmAmt` gave. The strings, drums \
               and `$mi.sixOp` shape their own notes and take `accent` in \
               place of `level`.",
        table: Some(DocTable {
            columns: &["Engine", "Use"],
            rows: &[
                &[
                    "`'vaVcf'`",
                    "`$saw` or `$pulse`, with a `$pulse` an octave down, into `$lpf` or `$multimode`",
                ],
                &["`'phaseDistortion'`", "`$mi.phaseDistortion`"],
                &["`'sixOpA'`, `'sixOpB'`, `'sixOpC'`", "`$mi.sixOp`"],
                &["`'waveTerrain'`", "`$mi.waveTerrain`"],
                &[
                    "`'stringMachine'`",
                    "`$chord` into `$mi.divideDown`, then `$lpf` and `$mi.chorus`",
                ],
                &[
                    "`'chiptune'`",
                    "`$pulse` playing `$chord`, or an arpeggio from `$cycle`",
                ],
                &[
                    "`'virtualAnalog'`",
                    "`$saw` and `$pulse`, detuned, with `sync`",
                ],
                &["`'waveshaping'`", "`$mi.waveshaper`"],
                &["`'twoOpFm'`", "`$mi.twoOp`"],
                &["`'granularFormant'`", "`$mi.formant` and `$mi.zOsc`"],
                &["`'additive'`", "`$mi.additive`"],
                &[
                    "`'wavetable'`",
                    "`$wavetable`, with a wavetable WAV of your own",
                ],
                &[
                    "`'chords'`",
                    "`$chord`, with `inversion` and `level`, into `$mi.divideDown`",
                ],
                &[
                    "`'speech'`",
                    "`$mi.formantVowel`, `$mi.samVowel`, `$mi.lpcVowel` and `$mi.speak`",
                ],
                &["`'swarm'`", "`$mi.swarm`"],
                &[
                    "`'filteredNoise'`",
                    "`$noise`, clocked by `rate`, into `$bpf` or `$multimode`",
                ],
                &["`'particleNoise'`", "`$mi.particles`"],
                &["`'inharmonicString'`", "`$mi.string`"],
                &["`'modalResonator'`", "`$mi.modal`"],
                &["`'bassDrum'`", "`$mi.kick` and `$mi.synthKick`"],
                &["`'snareDrum'`", "`$mi.snare` and `$mi.synthSnare`"],
                &["`'hiHat'`", "`$mi.hiHat` and `$mi.ringHat`"],
            ],
        }),
        examples: &[],
    }],
    examples: &[
        DocExample {
            title: "Struck modal melody",
            code: r#"
                const seq = $p('c4 g4 eb4 <bb4 f4>')
                $macro(seq, 'modalResonator', {
                  trigger: seq,
                  timbre: 1,
                  lpgDecay: 3,
                }).out()
            "#,
        },
        DocExample {
            title: "Chord stabs",
            code: r#"
                $macro('c3', 'chords', {
                  trigger: $clock._beatTrigger,
                  harmonics: 1,
                }).out()
            "#,
        },
    ],
};

#[module(
    name = "$macro",
    docs = MACRO_DOCS,
    args(freq, engine),
    has_init
)]
pub struct Plaits {
    outputs: PlaitsOutputs,
    params: PlaitsParams,
    state: PlaitsState,
    /// One heavy Plaits voice per active channel; built in `init`.
    channel_state: Box<[PlaitsChannelState]>,
}

/// Module-level state for the Plaits module.
struct PlaitsState {
    buffer_pos: usize,
    sample_rate: f32,
}

impl Default for PlaitsState {
    fn default() -> Self {
        Self {
            buffer_pos: BLOCK_SIZE, // Start exhausted to trigger initial render
            sample_rate: 0.0,
        }
    }
}

impl Plaits {
    /// Initialize the module with the given sample rate.
    /// Called once at construction time by the macro-generated constructor.
    fn init(&mut self, sample_rate: f32) {
        self.state.sample_rate = sample_rate;
        let num_channels = self.channel_count().max(1);
        self.channel_state = (0..num_channels)
            .map(|_| {
                let mut voice = Voice::new(BLOCK_SIZE, sample_rate);
                voice.init();
                PlaitsChannelState {
                    voice,
                    ..PlaitsChannelState::default()
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
    }

    fn update(&mut self, sample_rate: f32) {
        let num_channels = self.channel_count().max(1);

        // Track triggers on every sample to catch short pulses
        // Triggers can be as short as 1 sample, so we need to detect rising edges
        // and latch them until the next block render.
        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];
            let trigger_val = self.params.trigger.value_or(ch, 0.0);

            // Detect rising edge using Schmitt trigger for noise immunity
            if state.trigger_schmitt.process(trigger_val) {
                // Rising edge detected - latch high trigger value
                state.trigger_latch = true;
            }
        }

        // Render when buffer is exhausted
        if self.state.buffer_pos >= BLOCK_SIZE {
            self.render_block(num_channels, sample_rate);
            self.state.buffer_pos = 0;
        }

        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];
            // Output scaling: Plaits outputs ±1.0, scale to ±5V (inverted to match hardware)
            let main = -state.main_buffer[self.state.buffer_pos] * 5.0;
            let aux = -state.aux_buffer[self.state.buffer_pos] * 5.0;

            self.outputs.main.set(ch, main);
            self.outputs.aux.set(ch, aux);

            state.mix.update(
                self.params.mix.value_or(ch, 0.0).clamp(0.0, 5.0),
                self.params.mix.glide_mark(ch),
                sample_rate,
            );

            let mix = *state.mix / 5.0;

            self.outputs
                .mix
                .set(ch, (main * (mix - 1.0).abs()) + (aux * mix))
        }

        self.state.buffer_pos += 1;
    }

    fn render_block(&mut self, num_channels: usize, sample_rate: f32) {
        let block_rate = sample_rate / BLOCK_SIZE as f32;
        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];

            state.harmonics.update(
                self.params.harmonics.value_or(ch, 0.0).clamp(-5.0, 5.0),
                self.params.harmonics.glide_mark(ch),
                block_rate,
            );
            state.timbre.update(
                self.params.timbre.value_or(ch, 0.0).clamp(-5.0, 5.0),
                self.params.timbre.glide_mark(ch),
                block_rate,
            );
            state.morph.update(
                self.params.morph.value_or(ch, 0.0).clamp(-5.0, 5.0),
                self.params.morph.glide_mark(ch),
                block_rate,
            );
            state.lpg_color.update(
                self.params.lpg_color.value_or(ch, 2.5).clamp(0.0, 5.0),
                self.params.lpg_color.glide_mark(ch),
                block_rate,
            );
            state.lpg_decay.update(
                self.params.lpg_decay.value_or(ch, 2.5).clamp(0.0, 5.0),
                self.params.lpg_decay.glide_mark(ch),
                block_rate,
            );

            state.timbre_amt.update(
                self.params.timbre_amt.value_or(ch, 0.0),
                self.params.timbre_amt.glide_mark(ch),
                block_rate,
            );
            state.fm_amt.update(
                self.params.fm_amt.value_or(ch, 0.0),
                self.params.fm_amt.glide_mark(ch),
                block_rate,
            );
            state.morph_amt.update(
                self.params.morph_amt.value_or(ch, 0.0),
                self.params.morph_amt.glide_mark(ch),
                block_rate,
            );

            // Get engine index from enum
            let engine_index = self.params.engine.to_index();

            let patch = Patch {
                note: voct_to_midi(self.params.freq.get_value(ch)),
                harmonics: ((*state.harmonics + 5.0) / 10.0).clamp(0.0, 1.0),
                timbre: ((*state.timbre + 5.0) / 10.0).clamp(0.0, 1.0),
                morph: ((*state.morph + 5.0) / 10.0).clamp(0.0, 1.0),
                engine: engine_index,
                decay: (*state.lpg_decay / 5.0).clamp(0.0, 1.0),
                lpg_colour: (*state.lpg_color / 5.0).clamp(0.0, 1.0),
                // Modulation amounts (attenuverters)
                frequency_modulation_amount: (*state.fm_amt / 5.0).clamp(-1.0, 1.0),
                timbre_modulation_amount: (*state.timbre_amt / 5.0).clamp(-1.0, 1.0),
                morph_modulation_amount: (*state.morph_amt / 5.0).clamp(-1.0, 1.0),
            };

            // Build the Modulations struct
            // FM: ±5V range, scaled to ±1.0 then multiplied by ~6 (VCV convention)
            let fm_val = self.params.fm.value_or(ch, 0.0);
            let fm_mod = fm_val / 5.0 * 6.0;

            // Use latched trigger value to ensure short triggers aren't missed
            // The latch captures any rising edge that occurred since last block render
            let trigger_mod = if state.trigger_latch { 1.0 } else { 0.0 };

            // Level: 0-5V scaled to 0-1
            let level_val = self.params.level.value_or(ch, 0.0);
            let level_mod = (level_val / 8.0).clamp(0.0, 1.0);

            // Shoreline merges Plaits' knob + CV jack into a single port per
            // destination, so harmonics/timbre/morph have no concept of an
            // external modulation source: the glided signal feeds the
            // patch.* base values, and timbreAmt / morphAmt route the
            // internal trigger envelope into those destinations.
            let modulations = Modulations {
                engine: 0.0, // No CV modulation of engine
                note: 0.0,   // No additional note modulation
                frequency: fm_mod,
                harmonics: 0.0,
                timbre: 0.0,
                morph: 0.0,
                trigger: trigger_mod,
                level: level_mod,
                frequency_patched: !self.params.fm.is_disconnected(),
                timbre_patched: false,
                morph_patched: false,
                trigger_patched: !self.params.trigger.is_disconnected(),
                level_patched: !self.params.level.is_disconnected(),
            };

            // Render the voice
            state.voice.render(
                &patch,
                &modulations,
                &mut state.main_buffer,
                &mut state.aux_buffer,
            );

            // Clear trigger latch and max after rendering
            // This ensures the trigger is only processed once per event
            state.trigger_latch = false;
        }
    }
}

message_handlers!(impl Plaits {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Connect, SameIds, Signal};

    /// A patch-edit glide follows the firmware's `ONE_POLE(value, target, c)`,
    /// run once per block at 48 kHz, one block behind: the glide holds the
    /// pre-edit value for the block the edit lands in.
    #[test]
    fn knob_glides_match_firmware_pot_filters() {
        let block_rate = 48_000.0 / BLOCK_SIZE as f32;
        for (tau, c) in [(KNOB_TAU_FAST, 0.01f32), (KNOB_TAU_SLOW, 0.005)] {
            let before = PolySignal::mono(Signal::Volts(-5.0));
            let mut after = PolySignal::mono(Signal::Volts(5.0));
            after.mark_glides(Some(&before), 1, &SameIds);
            let mut glide = SwapGlide::with_time_constant(tau);
            glide.update(-5.0, before.glide_mark(0), block_rate);
            glide.update(5.0, after.glide_mark(0), block_rate);
            let mut pot = -5.0f32;
            for block in 0..4_000 {
                pot += c * (5.0 - pot);
                glide.update(5.0, after.glide_mark(0), block_rate);
                assert!(
                    (*glide - pot).abs() < 1e-3,
                    "c = {c}: block {block} glide {} vs firmware {pot}",
                    *glide
                );
            }
        }
    }
}
