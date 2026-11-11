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
struct BandpassFilterParams {
    #[input("the sound to filter", type = audio)]
    input: PolySignal,
    #[input(
        "centre frequency; tops out just under half the sample rate",
        type = frequency,
        range = (-3.71, ..),
    )]
    center: PolySignal,
    #[input(
        "higher narrows the band, and keeps narrowing past 5",
        type = control,
        default = 1.0,
        range = (0.0, ..),
        control_range = (0.0, 5.0),
    )]
    #[deserr(default)]
    resonance: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct BandpassFilterOutputs {
    #[output("output", "filtered signal", type = cv, default)]
    sample: PolyOutput,
}

const BPF_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonant bandpass filter",
    description: "Passes a band around the centre frequency and cuts both sides. Narrow it \
                  with resonance for vocal, wah-like tones, or sweep it over noise for wind \
                  and whooshes.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Auto-wah saw",
            code: "$saw('c2').bpf($sine('2hz').range('c4', 'c6'), 3).out()",
        },
        DocExample {
            title: "Whistling wind",
            code: r#"
                $noise('white')
                  .bpf($lorenz({ rate: 0.1 }).range('c5', 'c7'), 4.5)
                  .out()
            "#,
        },
    ],
};

#[module(
    name = "$bpf",
    docs = BPF_DOCS,
    patch_update,
    args(input, center, resonance)
)]
pub struct BandpassFilter {
    outputs: BandpassFilterOutputs,
    state: SvfFilterState,
    channel_state: Box<[SvfChannel]>,
    params: BandpassFilterParams,
}

impl BandpassFilter {
    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();
        resonant_svf::run(
            Response::Bandpass,
            channels,
            &self.params.input,
            &self.params.center,
            &self.params.resonance,
            1.0,
            sample_rate,
            &mut self.state,
            &mut self.channel_state,
            &mut self.outputs.sample,
        );
    }
}

impl crate::types::PatchUpdateHandler for BandpassFilter {
    fn on_patch_update(&mut self) {
        resonant_svf::on_patch_update(
            &self.params.center,
            &self.params.resonance,
            &mut self.state,
            &mut self.channel_state,
        );
    }
}

message_handlers!(impl BandpassFilter {});
