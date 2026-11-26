mod algorithms;
mod dx7_voice;
mod envelope;
mod lfo;
mod patch;
mod syx;
mod units;
mod voice;

pub use dx7_voice::Dx7Voice;
pub use syx::{SyxVoice, parse_syx};

use deserr::Deserr;
use schemars::JsonSchema;

use super::accent::{Accent, plaits_volts};
use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;
use crate::{
    dsp::{
        mi::core::{dsp::soft_clip, phase::limit_frequency, random::Random},
        oscillators::{FmMode, apply_fm},
        utils::{
            SchmittTrigger,
            rng::{hash_bipolar, seed_base},
        },
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use voice::{FmVoice, Rates};

/// Plaits' output gain for this engine, at ±5 V full scale, after its
/// `SoftClip(x / 4)` output stage.
const OUTPUT_GAIN: f32 = -5.0;
/// The output level at full accent, which with Plaits' own response to
/// accent makes a fully accented note peak twice as high as an unaccented one.
const ACCENT_TRIM: f32 = 2.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SixOpParams {
    #[input("the note's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "the DX7 voice: a `.syx` file, then `#` and a voice name to pick one of several; a held note keeps its voice until the next note-on"
    )]
    voice: Dx7Voice,
    #[input(
        "holds the note: the envelopes attack and decay while high and release when it falls; a new note restarts them from where they are. Unpatched, the note sounds continuously and `envelope` scrubs through its envelopes",
        type = gate,
    )]
    #[deserr(default)]
    gate: Option<PolySignal>,
    #[input(
        "the level of the modulating operators: darker (0) to brighter and harsher (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    brightness: Option<PolySignal>,
    #[input(
        "envelope speed, from snappy (0) to slow attacks and long tails (5), the release shortest around 1.5; unpatched gate, the point in the envelopes the note sits at",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    envelope: Option<PolySignal>,
    #[input(
        "how hard each note is played, read as the gate rises: up to twice as loud at 5 V, and brighter as the patch's velocity sensitivity sets; unpatched gate, raises the drone's level",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp); through-zero runs the operators backward",
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
struct SixOpOutputs {
    #[output("output", "the FM voice; unaccented, within ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const SIX_OP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Six-operator DX7 FM voice",
    description: "Mutable Instruments Plaits' six-operator FM: the DX7's \
                  algorithms, envelopes and LFO playing any voice from a DX7 \
                  `.syx` file, for electric pianos, basses, bells and brass. \
                  Plaits' 96 voices are built in under `builtin/dx7/plaits/`. \
                  Leave `gate` unpatched for a drone whose `envelope` picks \
                  the moment in the note it holds.",
    categories: &[ReferenceCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Electric piano",
            code: r#"
                const seq = $`c3 [eb3 g3] <bb3 f3> ~`
                melody: $mi.sixOp(seq, 'builtin/dx7/plaits/e-piano-1.syx', {
                  gate: seq,
                })
            "#,
        },
        DocExample {
            title: "Frozen pad",
            code: r#"
                bass: $mi.sixOp('c2', 'builtin/dx7/plaits/m1-pads.syx', {
                  envelope: 3,
                })
            "#,
        },
    ],
};

#[module(name = "$mi.sixOp", docs = SIX_OP_DOCS, args(freq, voice), has_init, patch_update)]
pub struct SixOp {
    outputs: SixOpOutputs,
    state: SixOpState,
    lane_state: Box<[Lane]>,
    params: SixOpParams,
}

#[derive(Default)]
struct SixOpState {
    sample_rate: f32,
    inv_sample_rate: f32,
    rates: Rates,
}

impl SixOpState {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            inv_sample_rate: 1.0 / sample_rate,
            rates: Rates::new(sample_rate),
        }
    }
}

/// The inputs for a sample, in volts, with the glide marks of the knobs that
/// glide.
struct Knobs {
    brightness: (f32, GlideMark),
    envelope: (f32, GlideMark),
    accent: (f32, GlideMark),
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

/// One lane: Plaits' `SixOpEngine` with one voice, a sample at a time.
#[derive(Clone, Copy, SeedSide)]
struct Lane {
    voice: FmVoice,
    trigger: SchmittTrigger,
    gate: bool,
    /// The LFO's generator for sample-and-hold, as Plaits draws its global one.
    #[seed_side(keep)]
    random: Random,
    brightness: SwapGlide,
    envelope: SwapGlide,
    accent: SwapGlide,
    accent_state: Accent,
    /// The last finite brightness, envelope and accent, which a NaN or
    /// infinite knob holds.
    held: [f32; 3],
    /// Whether `random` has its per-instance seed. Transfers with the
    /// lane, so its LFOs stay continuous across patch updates.
    #[seed_side(keep)]
    seeded: bool,
}

impl Default for Lane {
    fn default() -> Self {
        Self {
            voice: FmVoice::default(),
            trigger: SchmittTrigger::default(),
            gate: false,
            random: Random::new(0),
            brightness: SwapGlide::default(),
            envelope: SwapGlide::default(),
            accent: SwapGlide::default(),
            accent_state: Accent::default(),
            held: [
                SixOpParams::BRIGHTNESS_DEFAULT,
                SixOpParams::ENVELOPE_DEFAULT,
                plaits_volts(SixOpParams::ACCENT_DEFAULT),
            ],
            seeded: false,
        }
    }
}

impl Lane {
    /// One sample of the voice at `f0` cycles per sample through Plaits'
    /// output stage, before the output gain. `gate` is `None` when unpatched,
    /// to scrub the envelopes.
    #[inline(always)]
    fn step(
        &mut self,
        f0: f32,
        knobs: Knobs,
        gate: Option<bool>,
        voice_param: &Dx7Voice,
        state: &SixOpState,
    ) -> f32 {
        let sample_rate = state.sample_rate;
        let rates = &state.rates;
        let f0 = limit_frequency(f0, 0.5);
        let raw = [
            self.brightness
                .follow(knobs.brightness.0, knobs.brightness.1, sample_rate),
            self.envelope
                .follow(knobs.envelope.0, knobs.envelope.1, sample_rate),
            self.accent
                .follow(knobs.accent.0, knobs.accent.1, sample_rate),
        ];
        for (held, value) in self.held.iter_mut().zip(raw) {
            if value.is_finite() {
                *held = value;
            }
        }
        let (patch, id) = (voice_param.patch(), voice_param.id());
        let brightness = knob(self.held[0]);
        let envelope = knob(self.held[1]);
        let velocity = knob(self.held[2]);

        let voice = &mut self.voice;
        match gate {
            None => {
                // Plaits scrubs the LFO before the patch loads; it then
                // modulates with the loaded patch's depths.
                voice
                    .lfo
                    .scrub(2.0 * sample_rate * envelope, &mut self.random);
                voice.load_patch(patch, id, rates);
                voice.parameters.sustain = true;
                voice.parameters.gate = false;
                // A gate patched again later starts from low, so its first
                // high loads the voice then set.
                self.gate = false;
            }
            Some(high) => {
                if high && !self.gate {
                    voice.load_patch(patch, id, rates);
                    voice.lfo.reset();
                }
                self.gate = high;
                voice.lfo.step(&mut self.random);
                voice.parameters.sustain = false;
                voice.parameters.gate = high;
            }
        }
        let p = &mut voice.parameters;
        p.f0 = f0;
        p.velocity = velocity;
        p.brightness = brightness;
        p.envelope_control = envelope;
        let lfo = voice.lfo;
        voice.set_modulations(&lfo);

        soft_clip(voice.render(rates) * 0.25)
    }
}

impl SixOp {
    fn init(&mut self, sample_rate: f32) {
        self.state = SixOpState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        for (lane_index, lane) in self.lane_state[..lanes].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(lane_index),
                p.fm.value_or(lane_index, SixOpParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let gate_volts = p.gate.as_ref().map(|g| g.get_value(lane_index));
            let gate = gate_volts.map(|volts| lane.trigger.process_with_edge(volts).0);
            let accent = (
                p.accent.value_or(lane_index, SixOpParams::ACCENT_DEFAULT),
                p.accent.glide_mark(lane_index),
            );
            let mark = accent.1;
            let accent = lane
                .accent_state
                .read(accent, gate_volts, self.state.sample_rate);
            let knobs = Knobs {
                brightness: (
                    p.brightness
                        .value_or(lane_index, SixOpParams::BRIGHTNESS_DEFAULT),
                    p.brightness.glide_mark(lane_index),
                ),
                envelope: (
                    p.envelope
                        .value_or(lane_index, SixOpParams::ENVELOPE_DEFAULT),
                    p.envelope.glide_mark(lane_index),
                ),
                accent: (plaits_volts(accent), mark),
            };
            let out = lane.step(f0, knobs, gate, &p.voice, &self.state);
            let out =
                lane.accent_state
                    .apply(OUTPUT_GAIN * out, ACCENT_TRIM, self.state.sample_rate);
            self.outputs.sample.set(lane_index, out);
        }
    }
}

impl crate::types::PatchUpdateHandler for SixOp {
    /// Seeds the LFO generators of lanes that have none yet. Runs here,
    /// not in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same values.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (lane_index, lane) in self.lane_state.iter_mut().enumerate() {
            if !lane.seeded {
                let unit = (hash_bipolar(base, lane_index as u64) + 1.0) * 0.5;
                lane.random = Random::new((unit * f64::from(u32::MAX)) as u32);
                lane.seeded = true;
            }
        }
    }
}

message_handlers!(impl SixOp {});

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/plaits_voices/mod.rs"]
mod plaits_voices;

/// The builtin voices, for tests.
#[cfg(test)]
pub(crate) mod test_voices {
    pub use super::plaits_voices::PLAITS_VOICES;

    /// The file of a builtin voice path (one under `assets/builtin`).
    pub fn asset(path: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/builtin")
            .join(path)
    }
}
