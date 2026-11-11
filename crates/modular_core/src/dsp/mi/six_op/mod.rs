mod algorithms;
mod envelope;
mod lfo;
mod patch;
mod units;
mod voice;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        mi::core::{phase::limit_frequency, random::Random},
        oscillators::{FmMode, apply_fm},
        utils::{
            SchmittTrigger,
            rng::{hash_bipolar, seed_base},
        },
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::{GlideMark, SwapGlide},
};

use patch::{BANKS, PATCH_BANKS, PATCHES, Patch};
use voice::{FmVoice, Rates};

/// Plaits' output gain for this engine, at ±5 V full scale, after its
/// `SoftClip(x / 4)` output stage.
const OUTPUT_GAIN: f32 = -5.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SixOpParams {
    #[input("the note's pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "holds the note: the envelopes attack and decay while high and release when it falls; a new note restarts them from where they are. Unpatched, the note sounds continuously and `envelope` scrubs through its envelopes",
        type = gate,
    )]
    #[deserr(default)]
    gate: Option<PolySignal>,
    #[input(
        "which of Plaits' three banks of 32 DX7 patches: basses, leads and synths (0), keys, mallets and percussion (1), organs, pads, strings and brass (2)",
        type = control,
        default = 0.0,
        range = (0.0, 2.0),
    )]
    #[deserr(default)]
    bank: Option<PolySignal>,
    #[input(
        "which patch of the bank; a held note keeps its patch until the next note-on",
        type = control,
        default = 0.0,
        range = (0.0, 31.0),
    )]
    #[deserr(default)]
    patch: Option<PolySignal>,
    #[input(
        "the level of the modulating operators: darker (0) to brighter and harsher (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    brightness: Option<PolySignal>,
    #[input(
        "envelope speed, from slow attacks and long tails (0) to snappy (5), the release longest around 1.5; unpatched gate, the point in the envelopes the note sits at",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    envelope: Option<PolySignal>,
    #[input(
        "how hard each note is played: louder and brighter as the patch's velocity sensitivity sets",
        type = control,
        default = 4.0,
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
    #[output("output", "the FM voice", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

const SIX_OP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Six-operator DX7 FM voice",
    description: "Mutable Instruments Plaits' six-operator FM: the DX7's \
                  algorithms, envelopes and LFO playing Plaits' three banks of \
                  32 patches, for electric pianos, basses, bells and brass. \
                  Leave `gate` unpatched for a drone whose `envelope` picks \
                  the moment in the note it holds.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Electric piano",
            code: r#"
                const seq = $p('c3 [eb3 g3] <bb3 f3> ~').c
                $mi.sixOp(seq, { gate: seq, bank: 1 }).out()
            "#,
        },
        DocExample {
            title: "Frozen pad",
            code: "$mi.sixOp('c2', { bank: 2, patch: 11, envelope: 3 }).out()",
        },
    ],
};

#[module(name = "$mi.sixOp", docs = SIX_OP_DOCS, args(freq), has_init, patch_update)]
pub struct SixOp {
    outputs: SixOpOutputs,
    state: SixOpState,
    channel_state: Box<[Channel]>,
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
    bank: f32,
    patch: f32,
    brightness: (f32, GlideMark),
    envelope: (f32, GlideMark),
    accent: (f32, GlideMark),
}

#[inline(always)]
fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

/// Plaits' `SoftClip`.
#[inline(always)]
fn soft_clip(x: f32) -> f32 {
    if x < -3.0 {
        -1.0
    } else if x > 3.0 {
        1.0
    } else {
        x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
    }
}

/// One channel: Plaits' `SixOpEngine` with one voice, a sample at a time.
struct Channel {
    voice: FmVoice,
    trigger: SchmittTrigger,
    gate: bool,
    /// The LFO's generator for sample-and-hold, as Plaits draws its global one.
    random: Random,
    brightness: SwapGlide,
    envelope: SwapGlide,
    accent: SwapGlide,
    /// The last finite brightness, envelope and accent, which a NaN or
    /// infinite knob holds.
    held: [f32; 3],
    /// The last finite bank and patch selection.
    bank: usize,
    patch: usize,
    /// Whether `random` has its per-instance seed. Transfers with the
    /// channel, so its LFOs stay continuous across patch updates.
    seeded: bool,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            voice: FmVoice::default(),
            trigger: SchmittTrigger::default(),
            gate: false,
            random: Random::new(0),
            brightness: SwapGlide::default(),
            envelope: SwapGlide::default(),
            accent: SwapGlide::default(),
            held: [
                SixOpParams::BRIGHTNESS_DEFAULT,
                SixOpParams::ENVELOPE_DEFAULT,
                SixOpParams::ACCENT_DEFAULT,
            ],
            bank: 0,
            patch: 0,
            seeded: false,
        }
    }
}

/// A selector in volts as the nearest of `count` indices; `None` when NaN.
fn select(volts: f32, count: usize) -> Option<usize> {
    (!volts.is_nan()).then(|| volts.round().clamp(0.0, (count - 1) as f32) as usize)
}

impl Channel {
    /// One sample of the voice at `f0` cycles per sample through Plaits'
    /// output stage, before the output gain. `gate` is `None` when unpatched,
    /// to scrub the envelopes.
    #[inline(always)]
    fn step(&mut self, f0: f32, knobs: Knobs, gate: Option<bool>, state: &SixOpState) -> f32 {
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
        if let Some(bank) = select(knobs.bank, BANKS) {
            self.bank = bank;
        }
        if let Some(patch) = select(knobs.patch, PATCHES) {
            self.patch = patch;
        }
        let patch: &'static Patch = &PATCH_BANKS[self.bank][self.patch];
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
                voice.load_patch(patch, rates);
                voice.parameters.sustain = true;
                voice.parameters.gate = false;
            }
            Some(high) => {
                if high && !self.gate {
                    voice.load_patch(patch, rates);
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
        std::sync::LazyLock::force(&PATCH_BANKS);
        self.state = SixOpState::new(sample_rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        for (ch, channel) in self.channel_state[..channels].iter_mut().enumerate() {
            let f0 = apply_fm(
                p.freq.get_value(ch),
                p.fm.value_or(ch, SixOpParams::FM_DEFAULT),
                p.fm_mode,
            ) * self.state.inv_sample_rate;
            let gate = p
                .gate
                .as_ref()
                .map(|g| channel.trigger.process_with_edge(g.get_value(ch)).0);
            let knobs = Knobs {
                bank: p.bank.value_or(ch, SixOpParams::BANK_DEFAULT),
                patch: p.patch.value_or(ch, SixOpParams::PATCH_DEFAULT),
                brightness: (
                    p.brightness.value_or(ch, SixOpParams::BRIGHTNESS_DEFAULT),
                    p.brightness.glide_mark(ch),
                ),
                envelope: (
                    p.envelope.value_or(ch, SixOpParams::ENVELOPE_DEFAULT),
                    p.envelope.glide_mark(ch),
                ),
                accent: (
                    p.accent.value_or(ch, SixOpParams::ACCENT_DEFAULT),
                    p.accent.glide_mark(ch),
                ),
            };
            let out = channel.step(f0, knobs, gate, &self.state);
            self.outputs.sample.set(ch, OUTPUT_GAIN * out);
        }
    }
}

impl crate::types::PatchUpdateHandler for SixOp {
    /// Seeds the LFO generators of channels that have none yet. Runs here,
    /// not in `init`, because `init` runs in a stack slot that consecutive
    /// constructions share, so every instance would draw the same values.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, channel) in self.channel_state.iter_mut().enumerate() {
            if !channel.seeded {
                let unit = (hash_bipolar(base, ch as u64) + 1.0) * 0.5;
                channel.random = Random::new((unit * f64::from(u32::MAX)) as u32);
                channel.seeded = true;
            }
        }
    }
}

message_handlers!(impl SixOp {});

#[cfg(test)]
mod tests;
