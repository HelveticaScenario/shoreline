use std::ops::Range;

use crate::dsp::utils::{SchmittTrigger, sanitize};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct PercussionEnvelopeParams {
    #[input("rising edge restarts the envelope from its current level", type = trig)]
    trigger: PolySignal,
    #[input("how long the envelope takes to die away, in seconds", type = control, default = 0.1, range = (0.001, ..), control_range = (0.001, 10.0))]
    #[deserr(default)]
    decay: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PercussionEnvelopeOutputs {
    #[output("output", "the envelope", type = cv, default, range = (0.0, 5.0))]
    sample: PolyOutput,
}

/// Per-lane envelope state
#[derive(Default, Clone, Copy)]
struct LaneState {
    current_level: f32,
    trigger_schmitt: SchmittTrigger,
    in_attack: bool,
    /// The decay time the cached coefficient was computed for, and the
    /// coefficient.
    decay_cache: Option<(f32, f32)>,
}

const PERC_DOCS: ModuleDocs = ModuleDocs {
    summary: "Percussive decay envelope",
    description: "Jumps to full level on each trigger and dies away: the quickest way \
                  to make plucks, hits and hats.",
    categories: &[ModuleCategory::Envelope],
    sections: &[],
    examples: &[
        DocExample {
            title: "Noise hi-hat",
            code: r#"
                $noise('white')
                  .amp($perc($clock._beatTrigger, { decay: 0.05 }))
                  .out()
            "#,
        },
        DocExample {
            title: "Plucked bass",
            code: r#"
                const notes = $`c2 c2 eb2 g1`
                $saw(notes)
                  .amp($perc(notes, { decay: 0.2 }))
                  .out()
            "#,
        },
    ],
};

#[module(name = "$perc", docs = PERC_DOCS, args(trigger), block)]
pub struct PercussionEnvelope {
    outputs: PercussionEnvelopeOutputs,
    params: PercussionEnvelopeParams,
    lane_state: Box<[LaneState]>,
}

impl LaneState {
    /// Advance one sample and return the envelope output. `decay` is the decay
    /// time in seconds and `attack_step` the per-sample attack rise.
    #[inline(always)]
    fn step(&mut self, trigger: f32, decay: f32, attack_step: f32, sample_rate: f32) -> f32 {
        // Detect rising edge of trigger using Schmitt trigger for noise immunity
        if self.trigger_schmitt.process(trigger) {
            // Trigger detected - start attack phase (continue from current level for smooth re-trigger)
            self.in_attack = true;
        }

        if self.in_attack {
            self.current_level += attack_step;
            if self.current_level >= 1.0 {
                self.current_level = 1.0;
                self.in_attack = false;
            }
        } else if self.current_level > 0.00001 {
            self.current_level *= self.decay_coeff(decay, sample_rate);
        } else {
            self.current_level = 0.0;
        }
        self.current_level * 5.0
    }

    /// Per-sample multiplier that decays the level to ~0.001 over `decay`
    /// seconds: e^(-t/tau) with tau = decay / 6.9 (ln(1000) ≈ 6.9). Cached
    /// until `decay` changes; the sample rate is fixed for the state's life.
    #[inline(always)]
    fn decay_coeff(&mut self, decay: f32, sample_rate: f32) -> f32 {
        match self.decay_cache {
            Some((cached, coeff)) if cached == decay => coeff,
            _ => {
                let tau = decay.max(0.001) / 6.9;
                let coeff = (-1.0 / (tau * sample_rate)).exp();
                self.decay_cache = Some((decay, coeff));
                coeff
            }
        }
    }
}

/// Per-sample linear rise of the 1 ms attack.
#[inline(always)]
fn attack_step(sample_rate: f32) -> f32 {
    const ATTACK_TIME: f32 = 0.001;
    1.0 / (ATTACK_TIME * sample_rate)
}

impl PercussionEnvelope {
    fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        let attack_step = attack_step(sample_rate);
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            let trigger = p.trigger.get_value(lane);
            let decay = p
                .decay
                .value_or(lane, PercussionEnvelopeParams::DECAY_DEFAULT);
            self.outputs
                .sample
                .set(lane, state.step(trigger, decay, attack_step, sample_rate));
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut PercussionEnvelopeBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let PercussionEnvelopeParamsBlockInputs { trigger, decay } =
            p.block_inputs(range.clone(), lanes, &mut scratch)?;
        let attack_step = attack_step(sample_rate);
        for i in range {
            let (trigger, decay) = (trigger.row(i), decay.row(i));
            let slots = outputs
                .sample
                .slot_mut(i)
                .iter_mut()
                .zip(self.lane_state.iter_mut());
            for (lane, (out, state)) in slots.enumerate() {
                *out = sanitize(state.step(trigger[lane], decay[lane], attack_step, sample_rate));
            }
        }
        Some(())
    }
}

message_handlers!(impl PercussionEnvelope {});
