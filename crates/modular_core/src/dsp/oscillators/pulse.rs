use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;
use crate::{
    dsp::{
        oscillators::{FmMode, Landing, Resets, SyncRatios, apply_fm, sync_blep, sync_ratio},
        utils::{sanitize, wrap_phase},
    },
    poly::{PORT_MAX_LANES, PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
struct PulseOscillatorParams {
    #[input("oscillator pitch; with `sync` set, the pitch it restarts at", type = pitch)]
    freq: PolySignal,
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
    #[input("frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp)", type = audio)]
    #[deserr(default)]
    fm: Option<PolySignal>,
    #[input("how `fm` bends the frequency")]
    #[serde(default)]
    #[deserr(default)]
    fm_mode: FmMode,
    #[input(
        "hard sync: how many octaves above its restart pitch the wave runs; sweep it for sync tearing",
        type = control,
        range = (0.0, ..),
        control_range = (0.0, 4.0),
    )]
    #[deserr(default)]
    #[block_input(optional)]
    sync: Option<PolySignal>,
    #[input("each rising edge restarts the cycle", type = trig)]
    #[deserr(default)]
    #[block_input(optional)]
    restart: Option<PolySignal>,
    #[input(
        "shifts where in the cycle the wave is read, in cycles; wraps around",
        type = control,
        default = 0.0,
        range = (0.0, 1.0),
    )]
    #[deserr(default)]
    phase_offset: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PulseOscillatorOutputs {
    #[output("output", "the pulse wave", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy, SeedSide)]
struct PulseLaneState {
    phase: f32,
    width: SwapGlide,
    pwm: SwapGlide,
    resets: Resets,
    /// PolyBLEP residual carried into the next sample from a reset.
    blep_carry: f32,
    /// Whether the cycle reset on the last sample.
    restarted: bool,
}

const PULSE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Pulse wave oscillator",
    description: "A pulse wave that is hollow and woody as a square and grows \
                  nasal and reedy as the pulse narrows. Sweeping the width with \
                  a slow LFO gives the classic chorused PWM string sound.",
    categories: &[ReferenceCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "PWM chord pad",
            code: r#"
                chord: $pulse($chord('C3[m]'), {
                  pwm: $sine('0.3hz').range(-2, 2),
                }).lpf('c6')
            "#,
        },
        DocExample {
            title: "Plucked square bass",
            code: r#"
                const seq = $`c2 c2 eb2 g1`
                melody: $pulse(seq).lpf(
                  $perc(seq, { decay: 0.2 }).range('c3', 'c6'),
                  2,
                )
            "#,
        },
    ],
};

#[module(name = "$pulse", docs = PULSE_DOCS, args(freq), block)]
pub struct PulseOscillator {
    outputs: PulseOscillatorOutputs,
    lane_state: Box<[PulseLaneState]>,
    params: PulseOscillatorParams,
}

impl PulseLaneState {
    /// Advance the phase by `phase_increment` (cycles per sample at the
    /// note's pitch) and return the next output sample. `width` is this
    /// sample's glided width plus PWM, clamped to 0–5, `restart` its restart
    /// input and `sync_ratio` its sync, if connected.
    #[inline(always)]
    fn step(
        &mut self,
        phase_increment: f32,
        width: f32,
        offset: f32,
        restart: Option<f32>,
        sync_ratio: Option<f32>,
    ) -> f32 {
        // Pulse width (0.0 to 1.0, 0.5 is square wave)
        let pulse_width = (width / 5.0).clamp(0.01, 0.99);

        let (phase_increment, reset) = self.resets.step(
            self.phase,
            phase_increment,
            restart,
            sync_ratio,
            Landing::Next,
        );
        self.phase = wrap_phase(self.phase + phase_increment);

        // Phase offset shifts the read position without altering the
        // accumulator, so it never drifts.
        let read_offset = offset.rem_euclid(1.0);
        let read_phase = (self.phase + read_offset).rem_euclid(1.0);

        let naive_pulse = |p: f32| if p < pulse_width { 1.0 } else { -1.0 };

        // Naive pulse plus PolyBLEP at its own rising (phase 0) and falling
        // (phase = width) edges. A reset lands in the upcoming interval, so
        // these operate on the real, pre-reset phase. A reset band-limits the
        // jump it makes, which replaces an edge ahead on the sample it lands
        // and an edge just behind on the sample after it.
        let abs_phase_inc = phase_increment.abs();
        let (restarted, resetting) = (self.restarted, reset.is_some());
        let edge = |phase: f32| {
            let replaced = if phase < abs_phase_inc {
                restarted
            } else {
                resetting
            };
            if replaced {
                0.0
            } else {
                poly_blep_pulse(phase, abs_phase_inc)
            }
        };
        let mut body = naive_pulse(read_phase);
        body += edge(read_phase);
        body -= edge(if read_phase >= pulse_width {
            read_phase - pulse_width
        } else {
            read_phase - pulse_width + 1.0
        });

        let pending = self.blep_carry;
        self.blep_carry = 0.0;
        self.restarted = resetting;

        // A reset restarts the cycle, with a PolyBLEP placed at its subsample
        // point to band-limit the jump to where a pulse running this way
        // starts. The jump is zero when the pulse already stood there.
        let mut now = 0.0;
        if let Some(reset) = reset {
            let before = naive_pulse(read_phase);
            self.phase = reset.phase(phase_increment);
            let start = if phase_increment < 0.0 {
                (read_offset - f32::EPSILON).rem_euclid(1.0)
            } else {
                read_offset
            };
            let (n, carry) = sync_blep(naive_pulse(start) - before, reset.frac());
            now = n;
            self.blep_carry = carry;
            // The restarted cycle's own edges before the next sample, which
            // that sample leaves to the carry.
            let cycle_start = reset.cycle_start();
            for (edge, rise) in [(0.0, 2.0), (pulse_width, -2.0)] {
                let (ahead, jump) = if phase_increment < 0.0 {
                    ((read_offset - edge).rem_euclid(1.0), -rise)
                } else {
                    ((edge - read_offset).rem_euclid(1.0), rise)
                };
                let at = cycle_start + ahead / abs_phase_inc;
                if ahead > 0.0 && at < 1.0 {
                    let (n, carry) = sync_blep(jump, at);
                    now += n;
                    self.blep_carry += carry;
                }
            }
        }

        (body + pending + now) * 5.0
    }
}

impl PulseOscillator {
    /// Outside a glide, reads width and PWM raw, exactly as `process_block`
    /// does.
    fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            let width = state.width.follow(
                p.width.value_or(lane, PulseOscillatorParams::WIDTH_DEFAULT),
                p.width.glide_mark(lane),
                sample_rate,
            );
            let pwm = state.pwm.follow(
                p.pwm.value_or(lane, PulseOscillatorParams::PWM_DEFAULT),
                p.pwm.glide_mark(lane),
                sample_rate,
            );
            let pitch = p.freq.get_value(lane);
            let fm = p.fm.value_or(lane, 0.0);
            let increment = apply_fm(pitch, fm, p.fm_mode) / sample_rate;
            let offset = p
                .phase_offset
                .value_or(lane, PulseOscillatorParams::PHASE_OFFSET_DEFAULT);
            let restart = p.restart.as_ref().map(|restart| restart.get_value(lane));
            let ratio = p.sync.as_ref().map(|sync| sync_ratio(sync.get_value(lane)));
            let width = (width + pwm).clamp(0.0, 5.0);
            self.outputs
                .sample
                .set(lane, state.step(increment, width, offset, restart, ratio));
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut PulseOscillatorBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let p = &self.params;
        let gliding = self.lane_state[..lanes]
            .iter()
            .enumerate()
            .any(|(lane, state)| {
                state.width.will_glide(p.width.glide_mark(lane))
                    || state.pwm.will_glide(p.pwm.glide_mark(lane))
            });
        if gliding {
            return None;
        }
        let mut scratch = Default::default();
        let PulseOscillatorParamsBlockInputs {
            freq,
            width,
            pwm,
            fm,
            sync,
            restart,
            phase_offset: offset,
        } = p.block_inputs(range.clone(), lanes, &mut scratch)?;
        let mut ratios = SyncRatios::new(sync, range.start);
        // A pitch held for the whole range needs its exponential only once.
        let mut held_increments = [0.0f32; PORT_MAX_LANES];
        let held = freq.is_constant() && fm.is_constant();
        if held {
            let (freq, fm) = (freq.row(range.start), fm.row(range.start));
            for ((increment, &pitch), &fm) in held_increments.iter_mut().zip(freq).zip(fm) {
                *increment = apply_fm(pitch, fm, p.fm_mode) / sample_rate;
            }
        }
        let last = range.end - 1;
        for i in range {
            let (pitch, fm, offset) = (freq.row(i), fm.row(i), offset.row(i));
            let (width, pwm) = (width.row(i), pwm.row(i));
            let restart = restart.as_ref().map(|restart| restart.row(i));
            let ratios = ratios.at(i);
            let slots = outputs
                .sample
                .slot_mut(i)
                .iter_mut()
                .zip(self.lane_state.iter_mut());
            for (lane, (out, state)) in slots.enumerate() {
                let increment = if held {
                    held_increments[lane]
                } else {
                    apply_fm(pitch[lane], fm[lane], p.fm_mode) / sample_rate
                };
                let width = (width[lane] + pwm[lane]).clamp(0.0, 5.0);
                *out = sanitize(state.step(
                    increment,
                    width,
                    offset[lane],
                    restart.map(|restart| restart[lane]),
                    ratios[lane],
                ));
            }
        }
        let (width, pwm) = (width.row(last), pwm.row(last));
        for (lane, state) in self.lane_state[..lanes].iter_mut().enumerate() {
            state.width.settle(width[lane], p.width.glide_mark(lane));
            state.pwm.settle(pwm[lane], p.pwm.glide_mark(lane));
        }
        Some(())
    }
}

// PolyBLEP for pulse wave
fn poly_blep_pulse(phase: f32, phase_increment: f32) -> f32 {
    // Detect discontinuity at phase wrap (0.0)
    if phase < phase_increment {
        let t = phase / phase_increment;
        return t + t - t * t - 1.0;
    }
    // Detect discontinuity approaching 1.0
    else if phase > 1.0 - phase_increment {
        let t = (phase - 1.0) / phase_increment;
        return t * t + t + t + 1.0;
    }
    0.0
}

message_handlers!(impl PulseOscillator {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, Signal};

    fn make_pulse(freq: f32, sync: Option<f32>, restart: Option<PolySignal>) -> PulseOscillator {
        let params = PulseOscillatorParams {
            freq: PolySignal::mono(Signal::Volts(freq)),
            width: None,
            pwm: None,
            fm: None,
            fm_mode: FmMode::default(),
            sync: sync.map(|v| PolySignal::mono(Signal::Volts(v))),
            restart,
            phase_offset: None,
        };
        let mut outputs = PulseOscillatorOutputs::default();
        outputs.set_all_lanes(1);
        PulseOscillator {
            params,
            outputs,
            _lane_count: 1,
            _channels: 1,
            _block_index: Default::default(),
            lane_state: vec![PulseLaneState::default(); 1].into_boxed_slice(),
        }
    }

    fn render(osc: &mut PulseOscillator, n: usize) -> Vec<f32> {
        (0..n)
            .map(|_| {
                osc.update(48_000.0);
                osc.outputs.sample.get(0)
            })
            .collect()
    }

    /// Sync at 0 restarts each cycle exactly where it wraps anyway, so from
    /// the first sample, across restarts and with sync switched on partway,
    /// the wave is the free one.
    #[test]
    fn sync_at_zero_is_the_free_wave() {
        let render = |sync_from: Option<usize>| {
            let mut osc = make_pulse(2.0, None, None);
            (0..4000)
                .map(|i| {
                    osc.params.restart = Some(PolySignal::mono(Signal::Volts(if i % 1000 < 2 {
                        5.0
                    } else {
                        0.0
                    })));
                    if sync_from == Some(i) {
                        osc.params.sync = Some(PolySignal::mono(Signal::Volts(0.0)));
                    }
                    osc.update(48_000.0);
                    osc.outputs.sample.get(0)
                })
                .collect::<Vec<f32>>()
        };
        let free = render(None);
        for sync_from in [0, 1537] {
            for (i, (synced, free)) in render(Some(sync_from)).iter().zip(&free).enumerate() {
                assert!(
                    (synced - free).abs() < 1e-3,
                    "synced from {sync_from}, sample {i}: {synced} vs {free}"
                );
            }
        }
    }

    #[test]
    fn synced_output_stays_within_its_range() {
        for freq in [-2.0, 1.0, 3.5] {
            let mut osc = make_pulse(freq, Some(0.0), None);
            for i in 0..8000 {
                osc.params.sync = Some(PolySignal::mono(Signal::Volts(i as f32 / 1000.0)));
                osc.update(48_000.0);
                let v = osc.outputs.sample.get(0);
                assert!(
                    v.is_finite() && v.abs() <= 5.0 + 1e-3,
                    "{freq} V, sample {i}: {v}"
                );
            }
        }
    }

    #[test]
    fn restart_starts_the_cycle_over() {
        let mut osc = make_pulse(-2.0, None, Some(PolySignal::mono(Signal::Volts(0.0))));
        render(&mut osc, 700);
        osc.params.restart = Some(PolySignal::mono(Signal::Volts(5.0)));
        osc.update(48_000.0);
        assert_eq!(osc.lane_state[0].phase, 0.0);
    }

    #[test]
    fn a_restarted_cycle_band_limits_the_edges_it_crosses_at_once() {
        // Restarted a step short of its rising edge, the square crosses it
        // half a step later: the samples either side share its band-limiting,
        // and neither overshoots.
        let mut lane = PulseLaneState {
            phase: 0.3,
            ..Default::default()
        };
        let first = lane.step(0.01, 2.5, 0.995, Some(5.0), None);
        let second = lane.step(0.01, 2.5, 0.995, Some(5.0), None);
        assert!((first - 3.05).abs() < 1e-4, "{first}");
        assert!((second - 3.95).abs() < 1e-4, "{second}");
    }
}
