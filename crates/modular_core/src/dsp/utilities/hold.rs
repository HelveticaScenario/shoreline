use crate::dsp::utils::{GATE_HIGH_VOLTAGE, SchmittTrigger, min_gate_samples};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct HoldParams {
    #[input("rising edge opens the output gate", type = gate)]
    input: PolySignal,
    #[input(
        "how long the gate stays open, in seconds; a rising edge mid-hold restarts it",
        type = control,
        default = 0.05,
        range = (0.0, ..),
        control_range = (0.0, 10.0),
    )]
    #[deserr(default)]
    time: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct HoldOutputs {
    #[output("output", "high for `time` after each rising edge", type = gate, default)]
    sample: PolyOutput,
}

#[derive(Clone, Copy, Default)]
struct HoldChannelState {
    schmitt: SchmittTrigger,
    elapsed: f32,
    active: bool,
    /// Samples left in a retrigger gap, during which the output is held low.
    gap_remaining: u32,
    /// Whether the previous sample's output was high: a rising edge then needs
    /// a gap for downstream gate inputs to see a new edge.
    output_high: bool,
}

const HOLD_DOCS: ModuleDocs = ModuleDocs {
    summary: "Stretch triggers into gates",
    description: "Turns each trigger into a gate of a fixed length, however long the \
                  input stays high. A new trigger mid-hold restarts it with a brief \
                  drop, so envelopes downstream fire again.",
    categories: &[ModuleCategory::Gate],
    sections: &[],
    examples: &[DocExample {
        title: "Clock ticks held into notes",
        code: r#"
            $saw('c3')
              .amp(
                $adsr($hold($clock.o.beatTrigger, 0.15), {
                  release: 0.2,
                }),
              )
              .out()
        "#,
    }],
};

#[module(name = "$hold", docs = HOLD_DOCS, args(input, time))]
pub struct Hold {
    outputs: HoldOutputs,
    params: HoldParams,
    channel_state: Box<[HoldChannelState]>,
}

impl Hold {
    fn update(&mut self, sample_rate: f32) {
        let sample_period = 1.0 / sample_rate;
        let retrigger_gap = min_gate_samples(sample_rate);
        for ch in 0..self.channel_count() {
            let state = &mut self.channel_state[ch];
            let time = self.params.time.value_or(ch, 0.05).max(0.0);
            let (_, edge) = state
                .schmitt
                .process_with_edge(self.params.input.get_value(ch));
            if edge.is_rising() {
                if state.output_high {
                    state.gap_remaining = retrigger_gap;
                }
                state.elapsed = 0.0;
                state.active = true;
            }
            if state.active && state.elapsed < time {
                state.elapsed += sample_period;
                state.output_high = state.gap_remaining == 0;
                if state.gap_remaining > 0 {
                    state.gap_remaining -= 1;
                }
            } else {
                state.active = false;
                state.output_high = false;
            }
            self.outputs.sample.set(
                ch,
                if state.output_high {
                    GATE_HIGH_VOLTAGE
                } else {
                    0.0
                },
            );
        }
    }
}

message_handlers!(impl Hold {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, Signal};

    const SR: f32 = 48_000.0;

    fn make(input_volts: f32, time_secs: Option<f32>) -> Hold {
        let mut outputs = HoldOutputs::default();
        outputs.set_all_channels(1);
        Hold {
            params: HoldParams {
                input: PolySignal::mono(Signal::Volts(input_volts)),
                time: time_secs.map(|t| PolySignal::mono(Signal::Volts(t))),
            },
            outputs,
            _channel_count: 1,
            _block_index: Default::default(),
            channel_state: vec![HoldChannelState::default(); 1].into_boxed_slice(),
        }
    }

    fn set_input(hold: &mut Hold, volts: f32) {
        hold.params.input = PolySignal::mono(Signal::Volts(volts));
    }

    /// Run `n` samples, returning how many produced a high (5V) output.
    fn count_high(hold: &mut Hold, n: usize) -> usize {
        let mut high = 0;
        for _ in 0..n {
            hold.update(SR);
            if hold.outputs.sample.get(0) >= GATE_HIGH_VOLTAGE - 0.01 {
                high += 1;
            }
        }
        high
    }

    fn assert_close(actual: usize, expected: usize) {
        assert!(
            actual.abs_diff(expected) <= 2,
            "expected ~{expected} high samples, got {actual}"
        );
    }

    #[test]
    fn rising_edge_opens_gate_for_time() {
        let mut hold = make(0.0, Some(0.01));
        assert_eq!(count_high(&mut hold, 8), 0);
        set_input(&mut hold, 5.0);
        assert_close(count_high(&mut hold, 1000), 480);
    }

    #[test]
    fn output_low_after_hold_even_while_input_high() {
        let mut hold = make(0.0, Some(0.01));
        set_input(&mut hold, 5.0);
        count_high(&mut hold, 1000);
        assert_eq!(count_high(&mut hold, 100), 0);
    }

    /// Run `n` samples, returning the output of each.
    fn outputs(hold: &mut Hold, n: usize) -> Vec<f32> {
        (0..n)
            .map(|_| {
                hold.update(SR);
                hold.outputs.sample.get(0)
            })
            .collect()
    }

    #[test]
    fn retrigger_mid_hold_drops_low_for_a_gap_then_restarts_the_timer() {
        let mut hold = make(0.0, Some(0.01));
        set_input(&mut hold, 5.0);
        count_high(&mut hold, 100);
        set_input(&mut hold, 0.0);
        count_high(&mut hold, 10);
        set_input(&mut hold, 5.0);
        let out = outputs(&mut hold, 1000);
        let gap = min_gate_samples(SR) as usize;
        // The output falls so downstream gate inputs see a new rising edge…
        assert!(out[..gap].iter().all(|&v| v == 0.0), "{:?}", &out[..gap]);
        assert_eq!(out[gap], GATE_HIGH_VOLTAGE);
        // …and the gate still closes `time` after the new edge.
        let high = out
            .iter()
            .filter(|&&v| v >= GATE_HIGH_VOLTAGE - 0.01)
            .count();
        assert_close(high, 480 - gap);
    }

    /// A 1-sample trigger followed by `low` samples of low input.
    fn pulse_then_low(low: usize) -> (Hold, Vec<f32>) {
        let mut hold = make(0.0, Some(0.01));
        set_input(&mut hold, 5.0);
        let mut out = outputs(&mut hold, 1);
        set_input(&mut hold, 0.0);
        out.extend(outputs(&mut hold, low));
        (hold, out)
    }

    #[test]
    fn a_trigger_on_the_sample_a_hold_would_close_still_gets_a_gap() {
        let (_, out) = pulse_then_low(1000);
        let gate_len = out.iter().take_while(|&&v| v == GATE_HIGH_VOLTAGE).count();

        // Retrigger on sample `gate_len`: the first one the gate would be low.
        let (mut hold, _) = pulse_then_low(gate_len - 1);
        set_input(&mut hold, 5.0);
        let gap = min_gate_samples(SR) as usize;
        let after = outputs(&mut hold, gap + 1);
        assert!(after[..gap].iter().all(|&v| v == 0.0), "{after:?}");
        assert_eq!(after[gap], GATE_HIGH_VOLTAGE);
    }

    #[test]
    fn retrigger_after_hold_ends_has_no_gap() {
        let mut hold = make(0.0, Some(0.01));
        set_input(&mut hold, 5.0);
        count_high(&mut hold, 10);
        set_input(&mut hold, 0.0);
        count_high(&mut hold, 1000);
        set_input(&mut hold, 5.0);
        assert_eq!(outputs(&mut hold, 1)[0], GATE_HIGH_VOLTAGE);
    }

    #[test]
    fn short_trigger_yields_full_gate() {
        let mut hold = make(0.0, Some(0.01));
        set_input(&mut hold, 5.0);
        let head = count_high(&mut hold, 3);
        set_input(&mut hold, 0.0);
        assert_close(head + count_high(&mut hold, 1000), 480);
    }

    #[test]
    fn default_time_holds_50ms() {
        let mut hold = make(0.0, None);
        set_input(&mut hold, 5.0);
        assert_close(count_high(&mut hold, 5000), 2400);
    }
}
