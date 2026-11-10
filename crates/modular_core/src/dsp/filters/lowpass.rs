use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::filters::resonant_svf::{self, Response, SvfChannel, SvfFilterState},
    poly::{PolyOutput, PolySignal},
};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct LowpassFilterParams {
    #[input("the sound to filter", type = audio)]
    input: PolySignal,
    #[input(
        "cutoff frequency; tops out just under half the sample rate",
        type = frequency,
        default = 0.0,
        range = (-3.71, ..),
    )]
    cutoff: PolySignal,
    #[input(
        "resonant peak at the cutoff; keeps sharpening past 5",
        type = control,
        default = 0.0,
        range = (0.0, ..),
        control_range = (0.0, 5.0),
    )]
    #[deserr(default)]
    resonance: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct LowpassFilterOutputs {
    #[output("output", "filtered signal", type = cv, default)]
    sample: PolyOutput,
}

const LPF_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonant lowpass filter",
    description: "Removes the highs above the cutoff: the everyday filter for taming bright \
                  oscillators and for envelope-swept basses and pads. Resonance adds a ringing \
                  peak at the cutoff but never self-oscillates; reach for `$jup6f` when you \
                  want the filter to sing.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Plucky acid bass",
            code: r#"
                const seq = $cycle($p('c2 c2 [eb2 c3] g1'))
                $saw(seq)
                  .$.lpf($perc(seq, { decay: 0.2 }).range('c3', 'c7'), 4)
                  .out()
            "#,
        },
        DocExample {
            title: "Slow pad sweep",
            code: r#"
                $saw($chord('C3[m7]'))
                  .$.lpf($sine('0.05hz').range('c4', 'c7'), 1)
                  .out()
            "#,
        },
    ],
};

#[module(
    name = "$lpf",
    docs = LPF_DOCS,
    patch_update,
    args(input, cutoff, resonance)
)]
pub struct LowpassFilter {
    outputs: LowpassFilterOutputs,
    state: SvfFilterState,
    channel_state: Box<[SvfChannel]>,
    params: LowpassFilterParams,
}

impl LowpassFilter {
    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();
        resonant_svf::run(
            Response::Lowpass,
            channels,
            &self.params.input,
            &self.params.cutoff,
            &self.params.resonance,
            0.0,
            sample_rate,
            &mut self.state,
            &mut self.channel_state,
            &mut self.outputs.sample,
        );
    }
}

impl crate::types::PatchUpdateHandler for LowpassFilter {
    fn on_patch_update(&mut self) {
        resonant_svf::on_patch_update(
            &self.params.cutoff,
            &self.params.resonance,
            &mut self.state,
            &mut self.channel_state,
        );
    }
}

message_handlers!(impl LowpassFilter {});
