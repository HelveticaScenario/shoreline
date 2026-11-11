mod ensemble;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{MonoSignal, MonoSignalExt, PolyOutput, PolySignal};
use crate::types::SwapGlide;

use ensemble::{Ensemble, EnsembleRates};

const AMOUNT_DEFAULT: f32 = 2.5;
const DEPTH_DEFAULT: f32 = 2.5;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct ChorusParams {
    #[input(
        "signal to thicken; even channels feed the left side, odd the right, and an odd last channel feeds both",
        type = audio,
    )]
    input: PolySignal,
    #[input(
        "how much chorus joins the dry signal, from none (0) to an even blend (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    amount: Option<MonoSignal>,
    #[input(
        "how far the slow and fast swirl sweeps, from a slight shimmer (0) to a deep, wobbling ensemble (5)",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    depth: Option<MonoSignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ChorusOutputs {
    #[output("output", "the chorused signal, as left and right channels", type = cv, default)]
    sample: PolyOutput,
}

const CHORUS_DOCS: ModuleDocs = ModuleDocs {
    summary: "Stereo ensemble chorus",
    description: "Mutable Instruments Plaits' ensemble: three delay taps \
                  per side, crossed between the sides and swept by a slow \
                  and a fast LFO, for the lush shimmer of a string \
                  machine. After `$mi.divideDown` and a low-pass it makes \
                  Plaits' string machine.",
    categories: &[ModuleCategory::Effect],
    sections: &[],
    examples: &[
        DocExample {
            title: "String machine",
            code: r#"
                const strings = $mi
                  .divideDown($chord('C3[m7]'), { registration: 1 })
                  .$.lpf('c6', 1)
                $mi.chorus(strings, { amount: 4 }).out()
            "#,
        },
        DocExample {
            title: "Wide pad",
            code: "$mi.chorus($saw('c3'), { amount: 5, depth: 5 }).out()",
        },
    ],
};

#[module(
    name = "$mi.chorus",
    docs = CHORUS_DOCS,
    channels = 2,
    has_init,
    args(input)
)]
pub struct Chorus {
    outputs: ChorusOutputs,
    state: ChorusState,
    params: ChorusParams,
}

#[derive(Default)]
struct ChorusState {
    sample_rate: f32,
    rates: EnsembleRates,
    ensemble: Ensemble,
    amount: SwapGlide,
    depth: SwapGlide,
    /// The last finite knobs, which a NaN or infinite knob holds.
    held: [f32; 2],
}

fn knob(volts: f32) -> f32 {
    (volts * 0.2).clamp(0.0, 1.0)
}

impl Chorus {
    fn init(&mut self, sample_rate: f32) {
        self.state.sample_rate = sample_rate;
        self.state.rates = EnsembleRates::new(sample_rate);
        self.state.ensemble = Ensemble::new(&self.state.rates);
        self.state.held = [AMOUNT_DEFAULT, DEPTH_DEFAULT];
    }

    fn update(&mut self, _sample_rate: f32) {
        let p = &self.params;
        let channels = p.input.channels();
        let (mut left, mut right) = (0.0, 0.0);
        for ch in 0..channels {
            // A non-finite sample would ring around the delay lines.
            let sample = p.input.get_value(ch);
            let sample = if sample.is_finite() { sample } else { 0.0 };
            if ch % 2 == 0 {
                left += sample;
            } else {
                right += sample;
            }
        }
        if channels % 2 == 1 {
            let sample = p.input.get_value(channels - 1);
            right += if sample.is_finite() { sample } else { 0.0 };
        }
        let sample_rate = self.state.sample_rate;
        let knobs = [
            self.state.amount.follow(
                p.amount.value_or(AMOUNT_DEFAULT),
                p.amount.glide_mark(),
                sample_rate,
            ),
            self.state.depth.follow(
                p.depth.value_or(DEPTH_DEFAULT),
                p.depth.glide_mark(),
                sample_rate,
            ),
        ];
        for (held, value) in self.state.held.iter_mut().zip(knobs) {
            if value.is_finite() {
                *held = value;
            }
        }
        let [amount, depth] = self.state.held.map(knob);
        let (left, right) =
            self.state
                .ensemble
                .process(left, right, amount, depth, &self.state.rates);
        self.outputs.sample.set(0, left);
        self.outputs.sample.set(1, right);
    }
}

message_handlers!(impl Chorus {});

#[cfg(test)]
mod tests;
