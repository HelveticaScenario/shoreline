//! `$pWarp` — applies a `$phase.*` warp to a phase signal.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::phase_warp::{PhaseWarp, PhaseWarpGlides, PhaseWarpState};
use crate::dsp::utils::voct_to_hz;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PORT_MAX_CHANNELS, PolyOutput, PolySignal, PolySignalExt};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct PWarpParams {
    #[input("phase to reshape, usually from `$ramp`", type = audio, range = (0.0, 1.0))]
    input: PolySignal,
    #[input("the warp to apply, built with `$phase.*`")]
    phase: PhaseWarp,
    #[input(
        "pitch of the incoming phase; patch it to cut aliasing from pulsar and feedback warps at high notes",
        type = pitch,
    )]
    #[deserr(default)]
    freq: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PWarpOutputs {
    #[output("output", "the reshaped phase", type = cv, default, range = (0.0, 1.0))]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct ChannelState {
    warp: PhaseWarpState,
    glides: PhaseWarpGlides,
}

/// As wide as the input, the warp's signals, and `freq`.
#[allow(private_interfaces)]
pub fn p_warp_derive_channel_count(params: &PWarpParams) -> usize {
    params
        .input
        .channels()
        .max(params.phase.channels())
        .max(params.freq.channel_count())
        .clamp(1, PORT_MAX_CHANNELS)
}

const P_WARP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Phase warp",
    description: "Reshapes a phase ramp with a `$phase.*` warp (mirror, bend, \
                  sync, fold, pwm, pulsar, crush or feedback) so the phase \
                  oscillator after it plays a new timbre. Chain warps for more \
                  complex shapes, and modulate their amounts for sweeping, \
                  vocal motion.",
    categories: &[ModuleCategory::Phase],
    sections: &[],
    examples: &[
        DocExample {
            title: "Pulsar sweep",
            code: r#"
                $pSine(
                  $pWarp(
                    $ramp('c3'),
                    $phase.pulsar($sine('0.1hz').range(0, 4)),
                    { freq: 'c3' },
                  ),
                ).out()
            "#,
        },
        DocExample {
            title: "Crushed feedback tone",
            code: r#"
                $pSine(
                  $pWarp($ramp('c3'), $phase.crush(2, $phase.feedback(1))),
                ).out()
            "#,
        },
    ],
};

#[module(name = "$pWarp", docs = P_WARP_DOCS, channels_derive = p_warp_derive_channel_count, args(input, phase))]
pub struct PWarp {
    outputs: PWarpOutputs,
    channel_state: Box<[ChannelState]>,
    params: PWarpParams,
}

impl PWarp {
    fn update(&mut self, sample_rate: f32) {
        let num_channels = self.channel_count();
        let freq_connected = !self.params.freq.is_disconnected();

        for ch in 0..num_channels {
            let freq_norm = if freq_connected {
                voct_to_hz(self.params.freq.value_or_zero(ch)) / sample_rate
            } else {
                0.0
            };
            let input = self.params.input.get_value(ch).clamp(0.0, 1.0);
            let state = &mut self.channel_state[ch];
            let resolved = self
                .params
                .phase
                .resolve(ch, sample_rate, &mut state.glides);
            let y = self
                .params
                .phase
                .evaluate(input, &resolved, freq_norm, &mut state.warp);
            self.outputs.sample.set(ch, y);
        }
    }
}

message_handlers!(impl PWarp {});
