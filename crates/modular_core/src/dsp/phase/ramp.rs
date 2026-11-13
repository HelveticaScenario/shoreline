//! Phase ramp generator module.
//!
//! Produces a phase ramp from 0 to 1 at a given frequency.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::{voct_to_hz, wrap_phase};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct RampParams {
    #[input("how fast the ramp cycles", type = pitch)]
    freq: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct RampOutputs {
    #[output("output", "phase rising once per cycle", type = cv, default, range = (0.0, 1.0))]
    sample: PolyOutput,
}

/// Per-lane phasor state
#[derive(Default, Clone, Copy)]
struct LaneState {
    phase: f32,
}

const RAMP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Phase ramp",
    description: "A ramp rising from 0 to 1 once per cycle: the phase source \
                  for `$pSine`, `$pSaw`, `$pPulse` and `$pWarp`. Phase \
                  oscillators sharing one ramp stay perfectly in step; at slow \
                  rates it is also a rising-sawtooth LFO.",
    categories: &[ModuleCategory::Phase, ModuleCategory::Modulation],
    sections: &[],
    examples: &[DocExample {
        title: "Locked sine and synced saw",
        code: r#"
            const phase = $ramp('c3')
            $pSine(phase).out()
            $pSaw($pWarp(phase, $phase.sync(2))).out()
        "#,
    }],
};

#[module(name = "$ramp", docs = RAMP_DOCS, args(freq))]
pub struct Ramp {
    outputs: RampOutputs,
    lane_state: Box<[LaneState]>,
    params: RampParams,
}

impl Ramp {
    fn update(&mut self, sample_rate: f32) {
        let num_lanes = self.lane_count();
        let inv_sample_rate = 1.0 / sample_rate;

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];

            let frequency = voct_to_hz(self.params.freq.get_value(lane));
            let phase_increment = frequency * inv_sample_rate;

            state.phase = wrap_phase(state.phase + phase_increment);

            self.outputs.sample.set(lane, state.phase);
        }
    }
}

message_handlers!(impl Ramp {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, Signal};

    fn make_ramp(freq_volts: f32) -> Ramp {
        let params = RampParams {
            freq: PolySignal::mono(Signal::Volts(freq_volts)),
        };
        let lanes = params.freq.lanes().max(1);
        let mut outputs = RampOutputs::default();
        outputs.set_all_lanes(lanes);
        Ramp {
            params,
            outputs,
            _lane_count: lanes,
            _channels: 1,
            _block_index: Default::default(),
            lane_state: vec![LaneState::default(); lanes].into_boxed_slice(),
        }
    }

    #[test]
    fn output_stays_in_range_above_twice_the_sample_rate() {
        // ~8.55 V is ≈ 96 kHz at C4 tuning, a phase increment of ~2 per
        // sample: the wrap must keep the output in [0, 1) for any increment.
        let mut ramp = make_ramp(8.55);
        for _ in 0..1000 {
            ramp.update(48_000.0);
            let v = ramp.outputs.sample.get(0);
            assert!((0.0..1.0).contains(&v), "output {v} must stay in [0, 1)");
        }
    }

    #[test]
    fn recovers_after_non_finite_frequency() {
        // 200 V overflows voct_to_hz to +inf; that must not stick in the
        // phase accumulator — once the pitch returns to a normal level the
        // ramp must rise again.
        let mut ramp = make_ramp(200.0);
        for _ in 0..8 {
            ramp.update(48_000.0);
        }
        assert!(
            ramp.lane_state[0].phase.is_finite(),
            "phase {} must stay finite through a non-finite frequency",
            ramp.lane_state[0].phase
        );

        ramp.params.freq = PolySignal::mono(Signal::Volts(0.0)); // C4
        let out: Vec<f32> = (0..100)
            .map(|_| {
                ramp.update(48_000.0);
                ramp.outputs.sample.get(0)
            })
            .collect();
        assert!(
            out.windows(2).all(|w| w[1] > w[0]),
            "ramp must rise monotonically at C4 once the input is normal"
        );
    }
}
