use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::rng::{LcgRng, seed_base};
use crate::dsp::utils::{GATE_HIGH_VOLTAGE, GATE_LOW_VOLTAGE, SchmittTrigger};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct CoinFlipParams {
    #[input("triggers or gates to flip", type = trig)]
    input: PolySignal,
    #[input(
        "chance of heads: 0 never, 2.5 half the time, 5 always",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    probability: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CoinFlipOutputs {
    #[output("heads", "triggers and gates that land heads", type = gate, default)]
    heads: PolyOutput,
    #[output("tails", "triggers and gates that land tails", type = gate)]
    tails: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct CoinFlipLaneState {
    rng: LcgRng,
    seeded: bool,
    input: SchmittTrigger,
    /// Outcome of the latest flip; routes the input gate until the next edge.
    heads: bool,
}

impl CoinFlipLaneState {
    /// Advance one sample and return the `(heads, tails)` voltages.
    /// `probability` is read only on a rising edge.
    #[inline]
    fn step(&mut self, input: f32, probability: impl FnOnce() -> f32) -> (f32, f32) {
        let (high, edge) = self.input.process_with_edge(input);
        if edge.is_rising() {
            let chance = probability().clamp(0.0, 5.0) / 5.0;
            // `next_unit` can round up to exactly 1.0, so certainty is checked
            // explicitly.
            self.heads = chance >= 1.0 || self.rng.next_unit() < chance;
        }
        let gate = |on: bool| {
            if on {
                GATE_HIGH_VOLTAGE
            } else {
                GATE_LOW_VOLTAGE
            }
        };
        (gate(high && self.heads), gate(high && !self.heads))
    }
}

const COIN_FLIP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Random trigger router",
    description: "Sends each trigger to heads or tails at random, weighted by the \
                  probability: thin a steady clock into a loose rhythm, or split one \
                  stream between two sounds. The module itself is the heads output; \
                  `._tails` carries the rest.",
    categories: &[ModuleCategory::Gate, ModuleCategory::Random],
    sections: &[],
    examples: &[DocExample {
        title: "Kick or snare, 75/25",
        code: r#"
            const flip = $coinFlip($clock._beatTrigger, 3.75)
            $tr808.kick(flip).out()
            $tr808.snare(flip._tails).out()
        "#,
    }],
};

#[module(
    name = "$coinFlip",
    docs = COIN_FLIP_DOCS,
    patch_update,
    args(input, probability)
)]
pub struct CoinFlip {
    outputs: CoinFlipOutputs,
    params: CoinFlipParams,
    lane_state: Box<[CoinFlipLaneState]>,
}

impl CoinFlip {
    fn update(&mut self, _sample_rate: f32) {
        for lane in 0..self.lane_count() {
            let (heads, tails) = self.lane_state[lane]
                .step(self.params.input.get_value(lane), || {
                    self.params.probability.value_or(lane, 2.5)
                });
            self.outputs.heads.set(lane, heads);
            self.outputs.tails.set(lane, tails);
        }
    }
}

impl crate::types::PatchUpdateHandler for CoinFlip {
    /// Seeds each unseeded lane off the module's stable heap address. The
    /// per-lane `seeded` flag rides `transfer_state_from`, so a running
    /// lane keeps its sequence across patch updates.
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            if !state.seeded {
                state.rng.seed(base, lane);
                state.seeded = true;
            }
        }
    }
}

message_handlers!(impl CoinFlip {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly::PolySignal;
    use crate::types::{OutputStruct, PatchUpdateHandler, Signal};

    const SR: f32 = 48_000.0;

    fn volts(v: f32) -> PolySignal {
        PolySignal::mono(Signal::Volts(v))
    }

    fn make(params: CoinFlipParams) -> CoinFlip {
        let lanes = __coin_flip_derive_lane_count(&params);
        let mut outputs = CoinFlipOutputs::default();
        outputs.set_all_lanes(lanes);
        let mut m = CoinFlip {
            params,
            outputs,
            lane_state: vec![CoinFlipLaneState::default(); lanes].into_boxed_slice(),
            _lane_count: lanes,
            _block_index: Default::default(),
        };
        crate::dsp::utils::rng::TEST_SEED_BASE.set(Some(0x5eed));
        m.on_patch_update();
        crate::dsp::utils::rng::TEST_SEED_BASE.set(None);
        m
    }

    /// Fire `n` two-sample triggers, each followed by two low samples, and
    /// return per-lane counts of `(heads, tails)` rising edges.
    fn flip(m: &mut CoinFlip, n: usize) -> Vec<(usize, usize)> {
        let lanes = m.lane_count();
        let mut counts = vec![(0, 0); lanes];
        let mut prev = vec![(0.0, 0.0); lanes];
        for i in 0..n * 4 {
            m.params.input = volts(if i % 4 < 2 { 5.0 } else { 0.0 });
            m.update(SR);
            for lane in 0..lanes {
                let now = (m.outputs.heads.get(lane), m.outputs.tails.get(lane));
                assert!(
                    now.0 == 0.0 || now.1 == 0.0,
                    "both outputs high on lane {lane}"
                );
                counts[lane].0 += usize::from(now.0 > prev[lane].0);
                counts[lane].1 += usize::from(now.1 > prev[lane].1);
                prev[lane] = now;
            }
        }
        counts
    }

    fn params(probability: PolySignal) -> CoinFlipParams {
        CoinFlipParams {
            input: volts(0.0),
            probability: Some(probability),
        }
    }

    #[test]
    fn every_trigger_lands_on_exactly_one_side() {
        let mut m = make(params(volts(2.5)));
        let [(heads, tails)] = flip(&mut m, 1000)[..] else {
            unreachable!()
        };
        assert_eq!(heads + tails, 1000);
        assert!((400..600).contains(&heads), "{heads} heads of 1000");
    }

    #[test]
    fn probability_endpoints_are_certain() {
        let mut never = make(params(volts(0.0)));
        assert_eq!(flip(&mut never, 500), [(0, 500)]);
        let mut always = make(params(volts(5.0)));
        assert_eq!(flip(&mut always, 500), [(500, 0)]);
    }

    #[test]
    fn probability_scales_heads_rate() {
        let mut m = make(params(volts(1.0)));
        let [(heads, _)] = flip(&mut m, 2000)[..] else {
            unreachable!()
        };
        assert!((320..480).contains(&heads), "{heads} heads of 2000");
    }

    #[test]
    fn default_probability_is_even() {
        let mut m = make(CoinFlipParams {
            input: volts(0.0),
            probability: None,
        });
        let [(heads, _)] = flip(&mut m, 1000)[..] else {
            unreachable!()
        };
        assert!((400..600).contains(&heads), "{heads} heads of 1000");
    }

    #[test]
    fn heads_follows_input_gate_length() {
        let mut m = make(params(volts(5.0)));
        m.params.input = volts(5.0);
        for _ in 0..100 {
            m.update(SR);
            assert_eq!(m.outputs.heads.get(0), GATE_HIGH_VOLTAGE);
        }
        m.params.input = volts(0.0);
        m.update(SR);
        assert_eq!(m.outputs.heads.get(0), GATE_LOW_VOLTAGE);
    }

    #[test]
    fn lanes_flip_independently() {
        let mut m = make(params(PolySignal::poly(&vec![Signal::Volts(2.5); 4])));
        let lanes = m.lane_count();
        let mut outcomes = vec![Vec::new(); lanes];
        for _ in 0..64 {
            for (lane, &(heads, _)) in flip(&mut m, 1).iter().enumerate() {
                outcomes[lane].push(heads);
            }
        }
        for (a, b) in (0..lanes).flat_map(|a| (a + 1..lanes).map(move |b| (a, b))) {
            assert_ne!(outcomes[a], outcomes[b], "lanes {a} and {b} match");
        }
    }
}
