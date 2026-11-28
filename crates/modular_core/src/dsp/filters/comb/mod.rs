mod lane;

use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::reference_category::ReferenceCategory;

use lane::{CombLane, Controls, Rates};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct CombParams {
    #[input("the sound to comb", type = audio)]
    input: PolySignal,
    #[input(
        "the pitch the comb's teeth sit on",
        type = frequency,
        range = (-3.71, ..),
    )]
    freq: PolySignal,
    #[input(
        "how long the teeth ring; negative keeps only odd harmonics, for a hollow tone",
        type = control,
        default = 3.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    feedback: Option<PolySignal>,
    #[input(
        "darkens each repeat, so high harmonics fade first like a plucked string",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    damp: Option<PolySignal>,
    #[input(
        "ring for the same time on every note, about 1 second at `feedback` 3; off, low notes ring longer than high ones"
    )]
    #[serde(default)]
    #[deserr(default)]
    even_ring: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CombOutputs {
    #[output("output", "combed signal", type = cv, default)]
    sample: PolyOutput,
}

const COMB_DOCS: ModuleDocs = ModuleDocs {
    summary: "Tuned comb resonator",
    description: "A short delay fed back on itself, which rings at a pitch and its harmonics. \
                  It tunes noise and drums to notes, plucks strings from clicks, and swept \
                  slowly it flanges.",
    categories: &[ReferenceCategory::Filter, ReferenceCategory::Effect],
    sections: &[],
    examples: &[
        DocExample {
            title: "Noise tuned to a chord",
            code: r#"
                pad: $noise('white').comb(['c3', 'e3', 'g3'], {
                  feedback: 4.5,
                })
            "#,
        },
        DocExample {
            title: "A click plucks a string",
            code: r#"
                pluck: $pPulse($clock[0]).comb('e2', {
                  feedback: 4.8,
                  damp: 2,
                })
            "#,
        },
        DocExample {
            title: "Hollow, odd-harmonic ring",
            code: r#"
                tone: $pPulse($clock[0]).comb('a2', {
                  feedback: -4.5,
                  damp: 1,
                })
            "#,
        },
        DocExample {
            title: "Slow flanger",
            code: r#"
                bass: $saw('c2')
                  .comb($sine('0.1hz').range(-1, 2), { feedback: 3 })
                  .wet(2.5)
            "#,
        },
    ],
};

#[module(
    name = "$comb",
    docs = COMB_DOCS,
    args(input, freq),
    has_init,
    block
)]
pub struct Comb {
    outputs: CombOutputs,
    state: CombState,
    lane_state: Box<[CombLane]>,
    params: CombParams,
}

#[derive(Default)]
struct CombState {
    rates: Rates,
}

impl Comb {
    fn init(&mut self, sample_rate: f32) {
        self.state.rates = Rates::new(sample_rate);
        for lane in self.lane_state.iter_mut() {
            lane.allocate(&self.state.rates);
        }
    }

    fn marks(p: &CombParams, lane: usize) -> [crate::types::GlideMark; 3] {
        [
            p.freq.glide_mark(lane),
            p.feedback.glide_mark(lane),
            p.damp.glide_mark(lane),
        ]
    }

    fn update(&mut self, _sample_rate: f32) {
        let p = &self.params;
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            let controls = Controls {
                freq: p.freq.get_value(lane),
                feedback: p.feedback.value_or(lane, CombParams::FEEDBACK_DEFAULT),
                damp: p.damp.value_or(lane, CombParams::DAMP_DEFAULT),
            };
            let y = state.step(
                p.input.get_value(lane),
                controls,
                Self::marks(p, lane),
                p.even_ring,
                &self.state.rates,
            );
            self.outputs.sample.set(lane, y);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut CombBlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let mut scratch = Default::default();
        let inputs = self
            .params
            .block_inputs(range.clone(), self.lane_count(), &mut scratch)?;
        let p = &self.params;
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            let marks = Self::marks(p, lane);
            for i in range.clone() {
                let controls = Controls {
                    freq: inputs.freq.row(i)[lane],
                    feedback: inputs.feedback.row(i)[lane],
                    damp: inputs.damp.row(i)[lane],
                };
                let y = state.step(
                    inputs.input.row(i)[lane],
                    controls,
                    marks,
                    p.even_ring,
                    &self.state.rates,
                );
                outputs.sample.slot_mut(i)[lane] = y;
            }
        }
        Some(())
    }
}

message_handlers!(impl Comb {});

#[cfg(test)]
mod tests;
