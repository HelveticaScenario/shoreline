use std::ops::Range;

use super::*;
use crate::dsp::drums::input::{Held, Input, Live, each_slot};
use crate::dsp::drums::voice::rails;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Seeds each channel's noise from the module's [`seed_base`].
fn seed(base: u64, channels: &mut [Channel]) {
    for (ch, cs) in channels.iter_mut().enumerate() {
        cs.noise.seed(base, ch);
    }
}

/// The inputs every channel of a tom or conga reads.
struct TomInputs<'a, I> {
    parts: &'static Parts,
    sh: &'a TomShared,
    trigger: I,
    freq: I,
    decay: I,
}

impl<I: Input> ChannelInputs<TomDrum> for TomInputs<'_, I> {
    type Channel = Channel;

    fn voices(channel: &mut Channel) -> &mut Voices<Tom> {
        &mut channel.voices
    }

    #[inline(always)]
    fn inputs(&self, ch: usize, cs: &mut Channel) -> (f32, TomControls) {
        let network = cs
            .network
            .get((self.freq.at(ch), self.decay.at(ch)), |(freq, decay)| {
                Network::new(self.parts, freq, decay, self.sh.rest_hz)
            });
        let controls = TomControls {
            t: network.t,
            white: cs.noise.next(self.sh.noise_scale),
            level: network.level,
        };
        (self.trigger.at(ch), controls)
    }
}

impl TomState {
    /// Runs one sample of every channel of the drum built from `parts` on
    /// its `[trigger, accent, freq, decay]`, handing `out` each one's
    /// output.
    #[inline(always)]
    fn tick<I: Input>(
        &mut self,
        parts: &'static Parts,
        channels: &mut [Channel],
        restart: bool,
        hold: bool,
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
            parts,
            sh: &self.shared,
            trigger,
            freq,
            decay,
        };
        self.groups.run(&inputs, channels, &run, |ch, v| {
            out(ch, rails(v * parts.gain))
        });
    }
}

macro_rules! tom_module {
    (
        docs = $docs:path,
        name = $name:literal,
        ident = $Struct:ident,
        parts = $parts:ident,
        freq = $freq:literal,
        freq_doc = $freq_doc:literal,
        decay_doc = $decay_doc:literal $(,)?
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
                    "how hard the hit lands, read at each strike: up to twice as loud at 5 V, starting sharper",
                    type = control,
                    default = 0.0,
                    range = (0.0, 5.0),
                )]
                #[deserr(default)]
                accent: Option<PolySignal>,
                #[input($freq_doc, type = pitch, default = $freq)]
                #[deserr(default)]
                freq: Option<PolySignal>,
                #[input($decay_doc, type = control, default = 2.5, range = (0.0, 5.0))]
                #[deserr(default)]
                decay: Option<PolySignal>,
                #[input(
                    "start every hit from rest, fading out one still sounding over 2 ms; off, each hit builds on the last, as on the 808",
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
                    self.state.shared.init(&$parts, sample_rate);
                    self.state.groups = Groups::new();
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
                    self.state.tick(
                        &$parts,
                        &mut self.channel_state[..channels],
                        p.restart,
                        p.hold,
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
                            self.state.tick(
                                &$parts,
                                channel_state,
                                p.restart,
                                p.hold,
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

const LT808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 low tom",
    description: "The 808's low tom: a round, low ring that starts sharp and settles in \
                  pitch, over a soft puff of noise.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Booming tom groove",
        code: r#"
            $tr808.lowTom($p('c(3,8)').c, { decay: 4 }).out()
        "#,
    }],
};

tom_module! {
    docs = LT808_DOCS,
    name = "$tr808.lowTom",
    ident = Lt808,
    parts = LT,
    freq = -1.5959,
    freq_doc = "pitch; stock is ≈87 Hz; its level shifts as you tune far from stock",
    decay_doc = "how long the drum rings, from ≈0.21 s to ≈1.9 s; ≈0.63 s at 2.5, as on the 808",
}

const MT808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 mid tom",
    description: "The 808's mid tom: a round ring that starts sharp and settles in \
                  pitch, over a soft puff of noise.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Tom fill",
        code: r#"
            $tr808.highTom($p('~ ~ ~ ~ c c ~ ~').c).out()
            $tr808.midTom($p('~ ~ ~ ~ ~ ~ c ~').c).out()
            $tr808.lowTom($p('~ ~ ~ ~ ~ ~ ~ c').c).out()
        "#,
    }],
};

tom_module! {
    docs = MT808_DOCS,
    name = "$tr808.midTom",
    ident = Mt808,
    parts = MT,
    freq = -0.9623,
    freq_doc = "pitch; stock is ≈134 Hz; its level shifts as you tune far from stock",
    decay_doc = "how long the drum rings, from ≈0.13 s to ≈1.2 s; ≈0.4 s at 2.5, as on the 808",
}

const HT808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 high tom",
    description: "The 808's high tom: a round, high ring that starts sharp and settles \
                  in pitch, over a soft puff of noise.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Tuned tom melody",
        code: r#"
            const toms = $p('g3 ~ bb3 c4 ~ g3 f3 ~').c
            $tr808.highTom(toms, { freq: toms }).out()
        "#,
    }],
};

tom_module! {
    docs = HT808_DOCS,
    name = "$tr808.highTom",
    ident = Ht808,
    parts = HT,
    freq = -0.5216,
    freq_doc = "pitch; stock is ≈182 Hz; its level shifts as you tune far from stock",
    decay_doc = "how long the drum rings, from ≈0.1 s to ≈0.92 s; ≈0.31 s at 2.5, as on the 808",
}

const LC808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 low conga",
    description: "The 808's low conga, the lowest of the three: a warm, hollow ring that \
                  starts sharp and settles in pitch.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Two-conga pattern",
        code: r#"
            $tr808.lowConga($p('~ ~ c ~ ~ c c ~').c).out()
            $tr808.highConga($p('c ~ ~ c ~ ~ ~ ~').c).out()
        "#,
    }],
};

tom_module! {
    docs = LC808_DOCS,
    name = "$tr808.lowConga",
    ident = Lc808,
    parts = LC,
    freq = -0.4471,
    freq_doc = "pitch; stock is ≈192 Hz; its level shifts as you tune far from stock",
    decay_doc = "how long the drum rings, from ≈0.21 s to ≈1.9 s; ≈0.63 s at 2.5, as on the 808",
}

const MC808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 mid conga",
    description: "The 808's mid conga, the middle of the three: a hollow, woody ring that \
                  starts sharp and settles in pitch.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Congas and claves",
        code: r#"
            $tr808.midConga($p('c ~ c c ~ c ~ ~').c).out()
            $tr808.claves($p('c(3,8)').c).out()
        "#,
    }],
};

tom_module! {
    docs = MC808_DOCS,
    name = "$tr808.midConga",
    ident = Mc808,
    parts = MC,
    freq = 0.0792,
    freq_doc = "pitch; stock is ≈276 Hz; its level shifts as you tune far from stock",
    decay_doc = "how long the drum rings, from ≈0.1 s to ≈0.92 s; ≈0.31 s at 2.5, as on the 808",
}

const HC808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 high conga",
    description: "The 808's high conga, the highest of the three: a bright, hollow ring \
                  that starts sharp and settles in pitch.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Three-conga groove",
        code: r#"
            $tr808.highConga($p('c ~ ~ c ~ ~ ~ ~').c).out()
            $tr808.midConga($p('~ ~ c ~ ~ c ~ ~').c).out()
            $tr808.lowConga($p('~ ~ ~ ~ ~ ~ c c').c).out()
        "#,
    }],
};

tom_module! {
    docs = HC808_DOCS,
    name = "$tr808.highConga",
    ident = Hc808,
    parts = HC,
    freq = 0.629,
    freq_doc = "pitch; stock is ≈405 Hz; its level shifts as you tune far from stock",
    decay_doc = "how long the drum rings, from ≈0.1 s to ≈0.92 s; ≈0.31 s at 2.5, as on the 808",
}
