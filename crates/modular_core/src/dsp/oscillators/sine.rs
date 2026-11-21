use std::ops::Range;

use std::f32::consts::TAU;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        consts::{LUT_SINE, LUT_SINE_SIZE},
        oscillators::{FmMode, Landing, Resets, SyncRatios, apply_fm, sync_corner, sync_ratio},
        utils::{interpolate, sanitize, wrap_phase},
    },
    poly::{PORT_MAX_LANES, PolyOutput, PolySignal, PolySignalExt},
};
use deserr::Deserr;
use schemars::JsonSchema;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
struct SineOscillatorParams {
    #[input("oscillator pitch; with `sync` set, the pitch it restarts at", type = pitch)]
    freq: PolySignal,
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
        default = 0.0,
        range = (0.0, ..),
        control_range = (0.0, 3.0),
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
struct SineOscillatorOutputs {
    #[output("output", "the sine wave", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

/// Per-lane oscillator state
#[derive(Default, Clone, Copy, SeedSide)]
struct LaneState {
    phase: f32,
    resets: Resets,
    /// PolyBLEP residual carried into the next sample from a reset.
    blep_carry: f32,
}

const SINE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Sine wave oscillator",
    description: "The purest tone, a single sine with no overtones: sub bass, \
                  soft leads and the classic FM carrier or modulator. \
                  Through-zero FM from another oscillator turns it into bells \
                  and electric pianos.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Soft sine melody",
            code: r#"
                const seq = $`c4 e4 g4 <b4 a4>`
                melody: $sine(seq).amp($perc(seq, { decay: 0.3 }))
            "#,
        },
        DocExample {
            title: "FM bell",
            code: r#"
                lead: $sine('c4', { fm: $sine('c5').range(-3, 3) }).amp(
                  $perc($clock._beatTrigger, { decay: 1 }),
                )
            "#,
        },
    ],
};

#[module(name = "$sine", docs = SINE_DOCS, args(freq), block)]
pub struct SineOscillator {
    outputs: SineOscillatorOutputs,
    lane_state: Box<[LaneState]>,
    params: SineOscillatorParams,
}

impl LaneState {
    /// Advance the phase by `increment` (cycles per sample at the note's
    /// pitch) and return the next output sample. `restart` is this sample's
    /// restart input and `sync_ratio` its sync, if connected.
    #[inline(always)]
    fn step(
        &mut self,
        increment: f32,
        offset: f32,
        restart: Option<f32>,
        sync_ratio: Option<f32>,
    ) -> f32 {
        let (increment, reset) =
            self.resets
                .step(self.phase, increment, restart, sync_ratio, Landing::Next);
        self.phase = wrap_phase(self.phase + increment);

        // Phase offset shifts the read position without altering the
        // accumulator, so it never drifts.
        let read_phase = (self.phase + offset).rem_euclid(1.0);

        // Naive sample at the (pre-reset) phase, plus any residual carried
        // from a reset on the previous sample.
        let body = interpolate(LUT_SINE, read_phase, LUT_SINE_SIZE);
        let pending = self.blep_carry;
        self.blep_carry = 0.0;

        // A reset restarts the cycle from where the old one reaches it, with
        // a PolyBLEP for the jump and a PolyBLAMP for the change of slope
        // placed at its subsample point.
        let mut now = 0.0;
        if let Some(reset) = reset {
            let frac = reset.frac();
            self.phase = reset.phase(increment);
            let sine = |phase: f32| interpolate(LUT_SINE, phase.rem_euclid(1.0), LUT_SINE_SIZE);
            // The slope in output units per sample: the cosine, a quarter
            // cycle ahead of the sine.
            let slope = |phase: f32| TAU * sine(phase + 0.25) * increment;
            let (reached, start) = (read_phase + frac * increment, offset);
            let (n, carry) = sync_corner(
                sine(start) - sine(reached),
                slope(start) - slope(reached),
                frac,
                increment,
            );
            now = n;
            self.blep_carry = carry;
        }

        (body + pending + now) * 5.0
    }
}

impl SineOscillator {
    fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            let pitch = p.freq.get_value(lane);
            let fm = p.fm.value_or(lane, 0.0);
            let increment = apply_fm(pitch, fm, p.fm_mode) / sample_rate;
            let offset = p
                .phase_offset
                .value_or(lane, SineOscillatorParams::PHASE_OFFSET_DEFAULT);
            let restart = p.restart.as_ref().map(|restart| restart.get_value(lane));
            let ratio = p.sync.as_ref().map(|sync| sync_ratio(sync.get_value(lane)));
            self.outputs
                .sample
                .set(lane, state.step(increment, offset, restart, ratio));
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut SineOscillatorBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let SineOscillatorParamsBlockInputs {
            freq,
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
        for i in range {
            let (pitch, fm, offset) = (freq.row(i), fm.row(i), offset.row(i));
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
                *out = sanitize(state.step(
                    increment,
                    offset[lane],
                    restart.map(|restart| restart[lane]),
                    ratios[lane],
                ));
            }
        }
        Some(())
    }
}

message_handlers!(impl SineOscillator {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly::PolySignal;
    use crate::types::{OutputStruct, Signal};

    fn make_sine(fm_volts: f32, fm_mode: FmMode) -> SineOscillator {
        let params = SineOscillatorParams {
            freq: PolySignal::mono(Signal::Volts(0.0)), // C4 ≈ 261 Hz
            fm: Some(PolySignal::mono(Signal::Volts(fm_volts))),
            fm_mode,
            sync: None,
            restart: None,
            phase_offset: None,
        };
        let lanes = params.freq.lanes().max(1);
        let mut outputs = SineOscillatorOutputs::default();
        outputs.set_all_lanes(lanes);
        SineOscillator {
            params,
            outputs,
            _lane_count: lanes,
            _channels: 1,
            _block_index: Default::default(),
            lane_state: vec![LaneState::default(); lanes].into_boxed_slice(),
        }
    }

    #[test]
    fn recovers_after_non_finite_frequency() {
        // An exp-FM voltage large enough to overflow voct_to_hz to +inf must
        // not stick in the phase accumulator: once the FM input returns to a
        // normal level, the oscillator sounds again within one cycle.
        let mut osc = make_sine(121.0, FmMode::Exp);
        for _ in 0..8 {
            osc.update(48_000.0);
        }
        assert!(
            osc.lane_state[0].phase.is_finite(),
            "phase {} must stay finite through a non-finite frequency",
            osc.lane_state[0].phase
        );

        osc.params.fm = Some(PolySignal::mono(Signal::Volts(0.0)));
        // C4 period ≈ 183.5 samples: a healthy oscillator reaches a
        // substantial level well within one cycle.
        let recovered = (0..184).any(|_| {
            osc.update(48_000.0);
            osc.outputs.sample.get(0).abs() > 1.0
        });
        assert!(recovered, "output must recover within a cycle");
    }

    fn render(osc: &mut SineOscillator, n: usize) -> Vec<f32> {
        (0..n)
            .map(|_| {
                osc.update(48_000.0);
                osc.outputs.sample.get(0)
            })
            .collect()
    }

    fn synced(freq: f32, octaves: Option<f32>) -> SineOscillator {
        let mut osc = make_sine(0.0, FmMode::Exp);
        osc.params.freq = PolySignal::mono(Signal::Volts(freq));
        osc.params.sync = octaves.map(|v| PolySignal::mono(Signal::Volts(v)));
        osc
    }

    /// Sync at 0 restarts each cycle exactly where it wraps anyway, so from
    /// the first sample, across restarts and with sync switched on partway,
    /// the wave is the free one.
    #[test]
    fn sync_at_zero_is_the_free_wave() {
        let render = |sync_from: Option<usize>| {
            let mut osc = synced(2.0, None);
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
    fn a_synced_wave_repeats_at_the_note_period() {
        // 300 Hz: a 160-sample period.
        let freq = (300.0f32 / 261.625_58).log2();
        let out = render(&mut synced(freq, Some(1.7)), 2000);
        let free = render(&mut synced(freq, None), 2000);
        for i in 400..1840 {
            assert!(
                (out[i] - out[i + 160]).abs() < 0.02,
                "sample {i}: {} vs {}",
                out[i],
                out[i + 160]
            );
        }
        let differs = (400..2000).any(|i| (out[i] - free[i - 1]).abs() > 1.0);
        assert!(differs, "sync reshapes the wave");
    }

    #[test]
    fn restart_starts_the_cycle_where_the_trigger_crosses() {
        // A trigger from 0 to 5 V crosses its threshold a fifth of the way
        // through the sample, so the sample after the restart lands stands
        // four fifths of a step into the new cycle.
        let mut osc = synced(-2.0, None);
        osc.params.restart = Some(PolySignal::mono(Signal::Volts(0.0)));
        render(&mut osc, 700);
        osc.params.restart = Some(PolySignal::mono(Signal::Volts(5.0)));
        render(&mut osc, 2);
        let step = crate::dsp::utils::voct_to_hz(-2.0) / 48_000.0;
        let phase = osc.lane_state[0].phase;
        assert!((phase - 0.8 * step).abs() < 1e-6, "{phase}");
    }

    #[test]
    fn a_restart_rounds_the_corner_it_makes() {
        // Restarted halfway through a step as it falls through zero, the
        // sine turns sharply upward from the cycle's start: the samples
        // either side of the corner rise by the same PolyBLAMP share.
        let step = 0.01;
        let mut lane = LaneState {
            phase: 0.5 - 1.5 * step,
            ..Default::default()
        };
        // From 0 to 2 V, the trigger crosses its threshold halfway.
        let first = lane.step(step, 0.0, Some(2.0), None);
        let second = lane.step(step, 0.0, Some(2.0), None);
        let sine = |phase: f32| 5.0 * interpolate(LUT_SINE, phase, LUT_SINE_SIZE);
        // A sixth of the slope change times half cubed, faded by the step's
        // distance from Nyquist.
        let fade = (1.0 - 2.0 * step).powi(2);
        let share = 5.0 * 2.0 * TAU * step / 6.0 * 0.125 * fade;
        let (lift_first, lift_second) = (first - sine(0.5 - 0.5 * step), second - sine(0.5 * step));
        assert!((lift_first - share).abs() < 2e-4, "{lift_first} vs {share}");
        assert!(
            (lift_second - share).abs() < 2e-4,
            "{lift_second} vs {share}"
        );
    }

    #[test]
    fn a_synced_sine_stays_near_its_range_up_to_nyquist() {
        // Swept from the note to its speed limit, the corrections round each
        // corner without the overshoot a straight-line correction makes on a
        // wave that curves within a sample.
        for freq in [1.0, 3.0, 4.5] {
            let mut osc = synced(freq, Some(0.0));
            for i in 0..8000 {
                osc.params.sync = Some(PolySignal::mono(Signal::Volts(i as f32 / 1000.0)));
                osc.update(48_000.0);
                let v = osc.outputs.sample.get(0);
                assert!(v.abs() <= 5.75, "{freq} V, sample {i}: {v}");
            }
        }
    }
}
