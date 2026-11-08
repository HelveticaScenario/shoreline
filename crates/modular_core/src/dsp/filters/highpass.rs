use deserr::Deserr;
use schemars::JsonSchema;

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
    #[output("output", "filtered signal", default)]
    sample: PolyOutput,
}

/// Highpass filter that attenuates frequencies below the cutoff point.
///
/// Use it to remove low-end rumble, thin out a sound, or create rising
/// filter effects. Pairs well with lowpass filters for isolating a
/// frequency band.
///
/// - **cutoff** — set in V/Oct (0 V = C4). Accepts modulation for filter sweeps.
/// - **resonance** — boosts frequencies near the cutoff (0–5). High values
///   produce a ringing peak.
///
/// ```js
/// // remove low end from a noise source
/// $hpf($noise("white"), 'a3', 1)
/// ```
#[module(
    name = "$hpf",
    categories(filter),
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
