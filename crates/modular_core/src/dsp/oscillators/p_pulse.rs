use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::utils::wrap,
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
struct PPulseOscillatorParams {
    #[input(
        "phase to read the wave at, usually from `$ramp`: the range is one cycle; values outside wrap",
        type = audio,
        range = (0.0, 1.0),
    )]
    phase: PolySignal,
    #[input(
        "pulse width: 0 a thin high spike, 2.5 square, 5 a thin low notch",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    width: Option<PolySignal>,
    #[input(
        "pulse width modulation, added to `width`",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    pwm: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PPulseOscillatorOutputs {
    #[output("output", "the pulse wave", default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

/// Per-channel state for width glide and phase tracking
#[derive(Default, Clone, Copy)]
struct ChannelState {
    width: SwapGlide,
    pwm: SwapGlide,
    prev_phase: f32,
}

const P_PULSE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Phase-driven pulse",
    description: "The pulse wave of `$pulse`, following a phase input instead \
                  of its own pitch. Warp the phase with `$pWarp` to reshape the \
                  cycle before it is read.",
    categories: &[ModuleCategory::Oscillator, ModuleCategory::Phase],
    sections: &[],
    examples: &[DocExample {
        title: "Mirrored pulse sweep",
        code: "$pPulse($pWarp($ramp('c3'), $phase.mirror($sine('0.3hz').range(0, 5))), { width: 1 }).out()",
    }],
};

#[module(name = "$pPulse", docs = P_PULSE_DOCS, args(phase))]
pub struct PPulseOscillator {
    outputs: PPulseOscillatorOutputs,
    channel_state: Box<[ChannelState]>,
    params: PPulseOscillatorParams,
}

impl PPulseOscillator {
    fn update(&mut self, sample_rate: f32) {
        let num_channels = self.channel_count();

        for ch in 0..num_channels {
            let state = &mut self.channel_state[ch];

            state.width.update(
                self.params.width.value_or(ch, 2.5),
                self.params.width.glide_mark(ch),
                sample_rate,
            );
            state.pwm.update(
                self.params.pwm.value_or(ch, 0.0),
                self.params.pwm.glide_mark(ch),
                sample_rate,
            );
            let width = (*state.width + *state.pwm).clamp(0.0, 5.0);

            let phase = wrap(0.0..1.0, self.params.phase.get_value(ch));

            // Derive phase increment from external phasor
            let mut phase_increment = phase - state.prev_phase;
            if phase_increment < -0.5 {
                phase_increment += 1.0;
            } else if phase_increment > 0.5 {
                phase_increment -= 1.0;
            }
            phase_increment = phase_increment.abs().clamp(0.0, 0.5);

            state.prev_phase = phase;

            // Pulse width (0.0 to 1.0, 0.5 is square wave)
            let pulse_width = (width / 5.0).clamp(0.01, 0.99);

            // Naive pulse wave
            let mut pulse = if phase < pulse_width { 1.0 } else { -1.0 };

            // Apply PolyBLEP at the rising edge (phase = 0)
            pulse += poly_blep(phase, phase_increment);

            // Apply PolyBLEP at the falling edge (phase = pulse_width)
            pulse -= poly_blep(
                if phase >= pulse_width {
                    phase - pulse_width
                } else {
                    phase - pulse_width + 1.0
                },
                phase_increment,
            );

            self.outputs.sample.set(ch, pulse * 5.0);
        }
    }
}

/// PolyBLEP (Polynomial Band-Limited Step) correction.
/// Smooths step discontinuities by replacing the hard edge with a
/// polynomial residual within one sample of the transition.
#[inline(always)]
fn poly_blep(phase: f32, phase_increment: f32) -> f32 {
    if phase_increment < 1.0e-7 {
        return 0.0;
    }
    if phase < phase_increment {
        let t = phase / phase_increment;
        return t + t - t * t - 1.0;
    } else if phase > 1.0 - phase_increment {
        let t = (phase - 1.0) / phase_increment;
        return t * t + t + t + 1.0;
    }
    0.0
}

message_handlers!(impl PPulseOscillator {});
