use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        consts::{LUT_SINE, LUT_SINE_SIZE},
        utils::{interpolate, wrap},
    },
    poly::{PolyOutput, PolySignal},
};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
struct PSineOscillatorParams {
    #[input(
        "phase to read the wave at, usually from `$ramp`: the range is one cycle; values outside wrap",
        type = audio,
        range = (0.0, 1.0),
    )]
    phase: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PSineOscillatorOutputs {
    #[output("output", "the sine wave", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

const P_SINE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Phase-driven sine",
    description: "A sine that follows a phase input instead of running at its \
                  own pitch. Drive it from `$ramp` through `$pWarp` to bend the \
                  cycle into new timbres, or share one ramp between phase \
                  oscillators to keep them locked together.",
    categories: &[ModuleCategory::Oscillator, ModuleCategory::Phase],
    sections: &[],
    examples: &[DocExample {
        title: "Bending sine",
        code: r#"
            $pSine(
              $pWarp(
                $ramp('c3'),
                $phase.bend($sine('0.2hz').range(-3, 3)),
              ),
            ).out()
        "#,
    }],
};

#[module(name = "$pSine", docs = P_SINE_DOCS, args(phase))]
pub struct PSineOscillator {
    outputs: PSineOscillatorOutputs,
    params: PSineOscillatorParams,
}

impl PSineOscillator {
    fn update(&mut self, _sample_rate: f32) {
        let num_channels = self.channel_count();

        for ch in 0..num_channels {
            let phase = wrap(0.0..1.0, self.params.phase.get_value(ch));
            let sine = interpolate(LUT_SINE, phase, LUT_SINE_SIZE);
            self.outputs.sample.set(ch, sine * 5.0);
        }
    }
}

message_handlers!(impl PSineOscillator {});
