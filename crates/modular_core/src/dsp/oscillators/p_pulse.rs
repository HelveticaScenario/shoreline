use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        oscillators::waveforms::{pulse, pulse_width},
        utils::wrap,
    },
    poly::{PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};
use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::default_true;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
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
    #[input(
        "smooths the edges so high notes stay clean; off gives exact steps and corners, for LFOs or a raw aliased tone"
    )]
    #[serde(default = "default_true")]
    #[deserr(default = default_true())]
    bandlimit: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PPulseOscillatorOutputs {
    #[output("output", "the pulse wave", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

/// Per-lane state for width glide and phase tracking
#[derive(Default, Clone, Copy, SeedSide)]
struct LaneState {
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
        code: r#"
            tone: $pPulse(
              $pWarp(
                $ramp('c3'),
                $phase.mirror($sine('0.3hz').range(0, 5)),
              ),
              { width: 1 },
            )
        "#,
    }],
};

#[module(name = "$pPulse", docs = P_PULSE_DOCS, args(phase))]
pub struct PPulseOscillator {
    outputs: PPulseOscillatorOutputs,
    lane_state: Box<[LaneState]>,
    params: PPulseOscillatorParams,
}

impl PPulseOscillator {
    fn update(&mut self, sample_rate: f32) {
        let num_lanes = self.lane_count();

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];

            state.width.update(
                self.params.width.value_or(lane, 2.5),
                self.params.width.glide_mark(lane),
                sample_rate,
            );
            state.pwm.update(
                self.params.pwm.value_or(lane, 0.0),
                self.params.pwm.glide_mark(lane),
                sample_rate,
            );
            let width = (*state.width + *state.pwm).clamp(0.0, 5.0);

            let phase = wrap(0.0..1.0, self.params.phase.get_value(lane));

            // Derive phase increment from external phasor
            let mut phase_increment = phase - state.prev_phase;
            if phase_increment < -0.5 {
                phase_increment += 1.0;
            } else if phase_increment > 0.5 {
                phase_increment -= 1.0;
            }
            phase_increment = phase_increment.abs().clamp(0.0, 0.5);

            state.prev_phase = phase;

            let sample = pulse(
                phase,
                pulse_width(width),
                phase_increment,
                self.params.bandlimit,
            );
            self.outputs.sample.set(lane, sample * 5.0);
        }
    }
}

message_handlers!(impl PPulseOscillator {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly::PolySignal;
    use crate::types::{OutputStruct, Signal};

    fn render(bandlimit: bool) -> Vec<f32> {
        let params = PPulseOscillatorParams {
            phase: PolySignal::mono(Signal::Volts(0.0)),
            width: None,
            pwm: None,
            bandlimit,
        };
        let mut outputs = PPulseOscillatorOutputs::default();
        outputs.set_all_lanes(1);
        let mut osc = PPulseOscillator {
            params,
            outputs,
            _lane_count: 1,
            _channels: 1,
            _block_index: Default::default(),
            lane_state: vec![LaneState::default(); 1].into_boxed_slice(),
        };
        (0..200)
            .map(|i| {
                let phase = (i as f32 * 0.037).fract();
                osc.params.phase = PolySignal::mono(Signal::Volts(phase));
                osc.update(48_000.0);
                osc.outputs.sample.get(0)
            })
            .collect()
    }

    /// Without band-limiting every sample is fully high or fully low.
    #[test]
    fn unbandlimited_pulse_only_switches() {
        assert!(render(false).iter().all(|&v| v.abs() == 5.0));
        assert!(
            render(true).iter().any(|&v| v.abs() != 5.0),
            "band-limited edges are smoothed"
        );
    }
}
