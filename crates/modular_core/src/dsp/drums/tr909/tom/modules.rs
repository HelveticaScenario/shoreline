use std::ops::Range;

use super::*;
use crate::dsp::drums::input::{Cached, Held, Hold, Input, Live, each_slot};
use crate::dsp::drums::tr909::circuit::TRIG_SECS;
use crate::dsp::drums::voice::{Timing, knob, rails};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

fn noise_network(dt: f32) -> Mna<NOISE_UNKNOWNS, NOISE_CAPS, 0, 1> {
    let mut b = MnaBuilder::<NOISE_UNKNOWNS>::default();
    b.resistor(NOISE_IN, GROUND, 1.0 / STIFF)
        .resistor(NOISE_IN, N1, 22.0e3)
        .capacitor(N1, GROUND, 0.0047e-6)
        .capacitor(N1, N2, 0.0047e-6)
        .capacitor(N2, N3, 0.0047e-6)
        .resistor(N2, HIGH_OUT, 10.0e3)
        .resistor(N3, GROUND, 220.0e3)
        .opamp(N3, HIGH_OUT, HIGH_OUT, OP27)
        .capacitor(HIGH_OUT, N53, 0.047e-6);
    b.build(dt, [])
}

/// The tom at every width.
struct TomDrum;

impl Drum for TomDrum {
    type Shared = TomShared;
    type Circuit<L: Lanes> = Tom<L>;
    type Controls<L: Lanes> = TomControls<L>;
    type Sources<L: Lanes> = ();
    const RUNS_IN_QUADS: bool = true;
    const RUNS_IN_PAIRS: bool = true;
}

#[derive(Default)]
struct TomState {
    hold: Hold<1, 2>,
    shared: TomShared,
    groups: Groups<TomDrum>,
}

#[derive(Clone, Copy, Default)]
struct Channel {
    voices: Voices<Tom>,
    noise: Noise,
    rate: Cached<f32, f32>,
}

/// Sets up a tom of `parts` at `sample_rate`.
fn init(state: &mut TomState, parts: TomParts, sample_rate: f32) {
    let dt = 1.0 / sample_rate;
    state.groups = Groups::new();
    let sh = &mut state.shared;
    sh.parts = parts;
    sh.timing = Timing::new(TRIG_SECS, sample_rate);
    sh.dt = dt;
    sh.noise_rate = Noise::rate(sample_rate);
    let mut injected = [0.0f32; NOISE_UNKNOWNS];
    injected[NOISE_IN] = NOISE_MEAN * STIFF;
    sh.noise_rest = noise_network(DC_STEP).settle(0, &injected);
    sh.noise = noise_network(dt);
    sh.reset = parts.vco.map(|c| (-dt / (R_RESET * c)).exp());
    sh.fall16 = (-dt / (R60 * C16)).exp();
    sh.fall17 = (-dt / (R61 * C17)).exp();
}

/// The inputs every tom reads.
struct TomInputs<I> {
    trigger: I,
    freq: I,
    decay: I,
    /// The stock pitch, V/Oct.
    stock: f32,
    noise_rate: f32,
}

impl<I: Input> ChannelInputs<TomDrum> for TomInputs<I> {
    type Channel = Channel;

    fn voices(channel: &mut Channel) -> &mut Voices<Tom> {
        &mut channel.voices
    }

    #[inline(always)]
    fn inputs(&self, ch: usize, channel: &mut Channel) -> (f32, TomControls) {
        let controls = TomControls {
            rate: channel
                .rate
                .get(self.freq.at(ch), |f| (f - self.stock).exp2()),
            decay: 1.0 / (R111 + VR11 * knob(self.decay.at(ch))),
            noise: channel.noise.next(self.noise_rate),
        };
        (self.trigger.at(ch), controls)
    }
}

impl TomState {
    /// Runs one sample of every channel on its `[trigger, accent, freq,
    /// decay]`, with `stock` the tom's stock pitch, handing `out` each one's
    /// output.
    #[inline(always)]
    fn tick<I: Input>(
        &mut self,
        channels: &mut [Channel],
        restart: bool,
        hold: bool,
        stock: f32,
        [trigger, accent, freq, decay]: [I; 4],
        mut out: impl FnMut(usize, f32),
    ) {
        let [freq, decay] = self
            .hold
            .apply(hold, channels.len(), [trigger], [freq, decay]);
        let trigger = Held::Live(trigger);
        let run = Run {
            accent,
            restart,
            fade_samples: self.shared.timing.fade_samples,
            shared: &self.shared,
        };
        let inputs = TomInputs {
            trigger,
            freq,
            decay,
            stock,
            noise_rate: self.shared.noise_rate,
        };
        self.groups.run(&inputs, channels, &run, |ch, v| {
            out(ch, rails(v * OUTPUT_GAIN))
        });
    }
}

/// Seeds each channel's noise from the module's [`seed_base`].
fn seed(base: u64, channels: &mut [Channel]) {
    for (ch, cs) in channels.iter_mut().enumerate() {
        cs.noise.seed(base, ch);
    }
}

macro_rules! tom_module {
    (
        docs = $docs:path,
        name = $name:literal,
        ident = $Struct:ident,
        parts = $parts:ident,
        freq = $freq:literal,
        freq_doc = $freq_doc:literal $(,)?
    ) => {
        paste::paste! {
            #[params]
            #[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
            #[serde(rename_all = "camelCase")]
            #[deserr(rename_all = camelCase, deny_unknown_fields)]
            struct [<$Struct Params>] {
                #[input("rising edges strike the drum", type = trig)]
                trigger: PolySignal,
                #[input(
                    "how hard the hit lands, read at each strike: up to twice as loud at 5 V",
                    type = control,
                    default = 0.0,
                    range = (0.0, 5.0),
                )]
                #[deserr(default)]
                accent: Option<PolySignal>,
                #[input($freq_doc, type = pitch, default = $freq)]
                #[deserr(default)]
                freq: Option<PolySignal>,
                #[input(
                    "how long the drum rings",
                    type = control,
                    default = 2.5,
                    range = (0.0, 5.0),
                )]
                #[deserr(default)]
                decay: Option<PolySignal>,
                #[input(
                    "start every hit from rest, fading out one still sounding over 2 ms; off, each hit builds on the last, as on the 909",
                )]
                #[serde(default)]
                #[deserr(default)]
                restart: bool,
                #[input(
                    "sample the controls on each strike and hold them until the next, as accent always is, so changes reach only the hits after them",
                )]
                #[serde(default)]
                #[deserr(default)]
                hold: bool,
            }

            #[derive(Outputs, JsonSchema)]
            #[serde(rename_all = "camelCase")]
            struct [<$Struct Outputs>] {
                #[output("output", "drum output; an unaccented hit peaks near ±5 V", type = cv, default, range = (-10.0, 10.0))]
                sample: PolyOutput,
            }

            #[module(name = $name, docs = $docs, has_init, patch_update, args(trigger), block)]
            pub struct $Struct {
                outputs: [<$Struct Outputs>],
                params: [<$Struct Params>],
                state: TomState,
                channel_state: Box<[Channel]>,
            }

            impl $Struct {
                fn init(&mut self, sample_rate: f32) {
                    init(&mut self.state, $parts, sample_rate);
                }

                fn update(&mut self, _sample_rate: f32) {
                    let channels = self.channel_count();
                    let p = &self.params;
                    let inputs = [
                        Live::new(&p.trigger),
                        Live::or(&p.accent, [<$Struct Params>]::ACCENT_DEFAULT),
                        Live::or(&p.freq, [<$Struct Params>]::FREQ_DEFAULT),
                        Live::or(&p.decay, [<$Struct Params>]::DECAY_DEFAULT),
                    ];
                    let out = &mut self.outputs.sample;
                    self.state
                        .tick(
                            &mut self.channel_state[..channels],
                            p.restart,
                            p.hold,
                            [<$Struct Params>]::FREQ_DEFAULT,
                            inputs,
                            |ch, v| out.set(ch, v),
                        );
                }

                fn process_block(
                    &mut self,
                    outputs: &mut [<$Struct BlockOutputs>],
                    range: Range<usize>,
                    _sample_rate: f32,
                ) -> Option<()> {
                    let channels = self.channel_count();
                    let p = &self.params;
                    let mut scratch = Default::default();
                    let [<$Struct ParamsBlockInputs>] {
                        trigger,
                        accent,
                        freq,
                        decay,
                    } = p.block_inputs(range.clone(), channels, &mut scratch)?;
                    let channel_state = &mut self.channel_state[..channels];
                    each_slot(
                        &mut outputs.sample,
                        range,
                        [trigger, accent, freq, decay],
                        |inputs, mut slot| {
                            self.state
                                .tick(
                                    channel_state,
                                    p.restart,
                                    p.hold,
                                    [<$Struct Params>]::FREQ_DEFAULT,
                                    inputs,
                                    |ch, v| slot.set(ch, v),
                                )
                        },
                    );
                    Some(())
                }
            }

            impl crate::types::PatchUpdateHandler for $Struct {
                fn on_patch_update(&mut self) {
                    let base = seed_base(self);
                    seed(base, &mut self.channel_state);
                    let count = self.channel_count();
                    self.state
                        .groups
                        .layout::<TomInputs<&[f32]>>(&mut self.channel_state, count);
                }
            }

            message_handlers!(impl $Struct {});
        }
    };
}

const LT909_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-909 low tom",
    description: "The 909's low tom: a deep tone that bends down from the strike and \
                  grows gritty as it fades, over a puff of noise.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Tom fill",
        code: r#"
            $tr909.kick($p('c*4').c).out()
            $tr909.highTom($p('~ ~ ~ ~ ~ ~ c c').c).out()
            $tr909.midTom($p('~ ~ ~ ~ ~ ~ ~ [c ~]').c).out()
            $tr909.lowTom($p('~ ~ ~ ~ ~ ~ ~ [~ c]').c).out()
        "#,
    }],
};

tom_module! {
    docs = LT909_DOCS,
    name = "$tr909.lowTom",
    ident = Lt909,
    parts = LOW,
    freq = -2.0993,
    freq_doc = "pitch; stock is ≈61 Hz",
}

const MT909_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-909 mid tom",
    description: "The 909's mid tom: a tone that bends down from the strike and grows \
                  gritty as it fades, pitched between the low and high toms.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Rolling tom groove",
        code: r#"
            $tr909.kick($p('c*4').c).out()
            $tr909.midTom($p('~ c ~ c c ~ ~ c').c).out()
        "#,
    }],
};

tom_module! {
    docs = MT909_DOCS,
    name = "$tr909.midTom",
    ident = Mt909,
    parts = MID,
    freq = -1.8098,
    freq_doc = "pitch; stock is ≈75 Hz",
}

const HT909_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-909 high tom",
    description: "The 909's high tom: a tone that bends down from the strike and grows \
                  gritty as it fades, the highest and shortest of the three.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Tuned tom melody",
        code: r#"
            const toms = $p('c3 ~ eb3 f3 ~ c3 bb2 ~').c
            $tr909.highTom(toms, { freq: toms }).out()
        "#,
    }],
};

tom_module! {
    docs = HT909_DOCS,
    name = "$tr909.highTom",
    ident = Ht909,
    parts = HIGH,
    freq = -1.5143,
    freq_doc = "pitch; stock is ≈92 Hz",
}
