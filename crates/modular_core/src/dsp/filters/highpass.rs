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
struct HighpassFilterParams {
    #[input("the sound to filter", type = audio)]
    input: PolySignal,
    #[input(
        "cutoff frequency; tops out just under half the sample rate",
        type = frequency,
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
struct HighpassFilterOutputs {
    #[output("output", "filtered signal", type = cv, default)]
    sample: PolyOutput,
}

const HPF_DOCS: ModuleDocs = ModuleDocs {
    summary: "Resonant highpass filter",
    description: "Removes the lows below the cutoff: clears rumble, thins a part so it sits \
                  in a mix, or builds a rising sweep. Resonance adds a ringing peak at the cutoff.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Pad thinning out and back",
            code: r#"
                $saw($chord('C3[maj7]'))
                  .$.hpf($sine('0.1hz').range('c3', 'c6'), 2)
                  .out()
            "#,
        },
        DocExample {
            title: "Riser across each bar",
            code: r#"
                $noise('white')
                  .$.hpf($clock.o.ramp.range('c2', 'c9'), 2)
                  .out()
            "#,
        },
    ],
};

#[module(
    name = "$hpf",
    docs = HPF_DOCS,
    patch_update,
    args(input, cutoff, resonance)
)]
pub struct HighpassFilter {
    outputs: HighpassFilterOutputs,
    state: SvfFilterState,
    channel_state: Box<[SvfChannel]>,
    params: HighpassFilterParams,
}

impl HighpassFilter {
    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();
        resonant_svf::run(
            Response::Highpass,
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

impl crate::types::PatchUpdateHandler for HighpassFilter {
    fn on_patch_update(&mut self) {
        resonant_svf::on_patch_update(
            &self.params.cutoff,
            &self.params.resonance,
            &mut self.state,
            &mut self.channel_state,
        );
    }
}

message_handlers!(impl HighpassFilter {});
