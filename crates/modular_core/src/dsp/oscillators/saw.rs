use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::default_true;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        oscillators::{
            FmMode, apply_fm, sync_edge_fraction, sync_reset,
            waveforms::{naive_triangle, triangle, triangle_symmetry},
        },
        utils::{SchmittTrigger, sanitize, wrap_phase_f64},
    },
    poly::{PORT_MAX_LANES, PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
struct SawOscillatorParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "waveform: 0 saw, 2.5 triangle, 5 ramp, morphing in between",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    shape: Option<PolySignal>,
    #[input("frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp)", type = audio)]
    #[deserr(default)]
    fm: Option<PolySignal>,
    #[input("how `fm` bends the frequency")]
    #[serde(default)]
    #[deserr(default)]
    fm_mode: FmMode,
    #[input("hard sync: each rising edge restarts the cycle", type = trig)]
    #[deserr(default)]
    #[block_input(optional)]
    sync: Option<PolySignal>,
    #[input(
        "shifts where in the cycle the wave is read, in cycles; wraps around",
        type = control,
        default = 0.0,
        range = (0.0, 1.0),
    )]
    #[deserr(default)]
    phase_offset: Option<PolySignal>,
    #[input(
        "a new voice stays silent until its wave first crosses zero, so a phase offset cannot start it with a click"
    )]
    #[serde(default = "default_true")]
    #[deserr(default = default_true())]
    wait_for_zero_cross: bool,
    #[input(
        "smooths the edges so high notes stay clean; off gives exact steps and corners, for LFOs or a raw aliased tone"
    )]
    #[serde(default = "default_true")]
    #[deserr(default = default_true())]
    bandlimit: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SawOscillatorOutputs {
    #[output("output", "the saw/triangle/ramp wave", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

/// Per-lane oscillator state
#[derive(Default, Clone, Copy, SeedSide)]
struct LaneState {
    /// Phase accumulator in `[0, 1)`. Kept in f64 so the DPW differencing
    /// (which divides a difference of near-equal integral values by the phase
    /// increment) stays well-conditioned at low, LFO-rate frequencies.
    phase: f64,
    shape: SwapGlide,
    /// False until the voice has seeded its starting phase to a zero crossing
    /// (done on the first `update`). A voice whose state was carried over by
    /// `transfer_state_from` arrives with this already set, so it keeps its
    /// phase for continuity instead of re-seeding.
    seeded: bool,
    /// Whether the zero-cross mute has released. Stays false while a freshly
    /// started voice is held silent, then latches true at the first output
    /// zero crossing. Carried-over (already-sounding) voices arrive with this
    /// true, so they keep playing without re-muting.
    gate_open: bool,
    /// Previous (un-muted) output sample, used to detect the zero crossing that
    /// releases the mute.
    prev_out: f32,
    /// Edge detector for the sync input.
    sync_schmitt: SchmittTrigger,
    /// Previous sync-input sample, for subsample edge interpolation.
    sync_prev: f32,
    /// PolyBLEP residual carried into the next sample from a sync reset.
    blep_carry: f32,
}

const SAW_DOCS: ModuleDocs = ModuleDocs {
    summary: "Saw, triangle and ramp oscillator",
    description: "A bright sawtooth that morphs through a mellow triangle to a \
                  ramp: the workhorse for basses, leads and pads under a \
                  filter. Moving the shape slowly gives a breathing, \
                  filter-like sweep on its own.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Acid bass sweep",
            code: r#"
                const seq = $`c2 c2 [c3 c2] eb2`
                melody: $saw(seq).lpf($sine('0.25hz').range('c3', 'c6'), 4)
            "#,
        },
        DocExample {
            title: "Breathing triangle",
            code: "keys: $saw('a3', { shape: $sine('0.2hz').range(1, 4) })",
        },
    ],
};

#[module(name = "$saw", docs = SAW_DOCS, args(freq), block)]
pub struct SawOscillator {
    outputs: SawOscillatorOutputs,
    lane_state: Box<[LaneState]>,
    params: SawOscillatorParams,
}

impl LaneState {
    /// Advance the phase by `phase_increment` (cycles per sample) and return
    /// the next output sample. `shape` is this sample's glided shape and
    /// `sync` its sync input, if connected.
    #[inline(always)]
    fn step(
        &mut self,
        phase_increment: f64,
        shape: f32,
        offset: f32,
        sync: Option<f32>,
        wait_for_zero_cross: bool,
        bandlimit: bool,
    ) -> f32 {
        let s = triangle_symmetry(shape, bandlimit);

        // Phase offset shifts the read position without altering the
        // accumulator, so it never drifts.
        let read_offset = offset.rem_euclid(1.0) as f64;

        // Seed a fresh voice so its read position starts just before the
        // gentle zero crossing, then the mute below holds silence until it
        // arrives. The waveform crosses zero mid-rise (phase s/2) and
        // mid-fall (phase (1+s)/2); the crossing on the longer of the two
        // segments is the gentle one (the other is the steep, band-limited
        // edge). We start the read `read_offset` of a cycle before that
        // crossing, so the wait equals the phase offset — a small offset
        // gives a short wait, not nearly a whole cycle, while still honoring
        // the offset in the running phase. The running read adds read_offset
        // on top of the accumulator, so the seed subtracts twice it to land
        // the read at `zero_cross - read_offset`. Only fresh voices seed;
        // carried-over state keeps its phase (see `seeded`).
        if !self.seeded {
            let zero_cross = if s >= 0.5 { s * 0.5 } else { (1.0 + s) * 0.5 };
            self.phase = (zero_cross - 2.0 * read_offset).rem_euclid(1.0);
            self.seeded = true;
            // Arm the zero-cross mute. With no offset the read starts on the
            // crossing, so prev_out is 0 and the mute releases on the first
            // sample; an offset starts it earlier, holding silence until the
            // output reaches the crossing.
            self.gate_open = !wait_for_zero_cross;
            let read_start = (zero_cross - read_offset).rem_euclid(1.0);
            self.prev_out = naive_triangle(read_start, s) as f32 * 5.0;
        }

        let read_old = (self.phase + read_offset).rem_euclid(1.0);
        self.phase = wrap_phase_f64(self.phase + phase_increment);
        let read_phase = (self.phase + read_offset).rem_euclid(1.0);

        // The sync reset (below) lands in the upcoming interval, so the phase
        // advanced normally here and the DPW differentiation stays valid.
        let body = triangle(read_old, read_phase, phase_increment, s, bandlimit) as f32;

        let pending = self.blep_carry;
        self.blep_carry = 0.0;

        // Hard sync: a rising edge resets the phase, with a PolyBLEP placed
        // at the subsample crossing when band-limited.
        let mut now = 0.0;
        if let Some(v) = sync {
            if self.sync_schmitt.process(v) {
                let frac = sync_edge_fraction(self.sync_prev, v);
                let before = naive_triangle(read_phase, s);
                self.phase = 0.0;
                let after = naive_triangle(read_offset, s);
                let (n, carry) = sync_reset((after - before) as f32, frac, bandlimit);
                now = n;
                self.blep_carry = carry;
            }
            self.sync_prev = v;
        }

        let out = (body + pending + now) * 5.0;

        // Zero-cross mute: hold silence until the output crosses zero on its
        // gentle segment, then latch open. The waveform also crosses zero on
        // its steep edge — releasing there would itself click — so only the
        // crossing that matches the seed's longer segment counts: rising
        // (negative→positive) for saw-side shapes, falling for ramp-side.
        // Releasing on the gentle slope means the first audible sample is ~0.
        let crossed = if s >= 0.5 {
            self.prev_out <= 0.0 && out >= 0.0
        } else {
            self.prev_out >= 0.0 && out <= 0.0
        };
        let sample = if self.gate_open {
            out
        } else if crossed {
            self.gate_open = true;
            out
        } else {
            0.0
        };
        self.prev_out = out;
        sample
    }
}

/// Phase increment in cycles per sample for V/oct `pitch` with `fm` applied.
#[inline(always)]
fn phase_increment(pitch: f32, fm: f32, fm_mode: FmMode, sample_rate: f32) -> f64 {
    apply_fm(pitch, fm, fm_mode) as f64 * (1.0 / sample_rate as f64)
}

impl SawOscillator {
    /// Outside a glide, reads the shape raw, exactly as `process_block` does.
    fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            let shape = state.shape.follow(
                p.shape
                    .value_or(lane, SawOscillatorParams::SHAPE_DEFAULT)
                    .clamp(0.0, 5.0),
                p.shape.glide_mark(lane),
                sample_rate,
            );
            let pitch = p.freq.get_value(lane);
            let fm = p.fm.value_or(lane, 0.0);
            let increment = phase_increment(pitch, fm, p.fm_mode, sample_rate);
            let offset = p
                .phase_offset
                .value_or(lane, SawOscillatorParams::PHASE_OFFSET_DEFAULT);
            let sync = p.sync.as_ref().map(|sync| sync.get_value(lane));
            let sample = state.step(
                increment,
                shape,
                offset,
                sync,
                p.wait_for_zero_cross,
                p.bandlimit,
            );
            self.outputs.sample.set(lane, sample);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut SawOscillatorBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let p = &self.params;
        let gliding = self.lane_state[..lanes]
            .iter()
            .enumerate()
            .any(|(lane, state)| state.shape.will_glide(p.shape.glide_mark(lane)));
        if gliding {
            return None;
        }
        let mut scratch = Default::default();
        let SawOscillatorParamsBlockInputs {
            freq,
            shape,
            fm,
            sync,
            phase_offset: offset,
        } = p.block_inputs(range.clone(), lanes, &mut scratch)?;
        // A pitch held for the whole range needs its exponential only once.
        let mut held_increments = [0.0f64; PORT_MAX_LANES];
        let held = freq.is_constant() && fm.is_constant();
        if held {
            let (freq, fm) = (freq.row(range.start), fm.row(range.start));
            for ((increment, &pitch), &fm) in held_increments.iter_mut().zip(freq).zip(fm) {
                *increment = phase_increment(pitch, fm, p.fm_mode, sample_rate);
            }
        }
        let last = range.end - 1;
        for i in range {
            let (pitch, fm, shape, offset) = (freq.row(i), fm.row(i), shape.row(i), offset.row(i));
            let sync = sync.as_ref().map(|sync| sync.row(i));
            let slots = outputs
                .sample
                .slot_mut(i)
                .iter_mut()
                .zip(self.lane_state.iter_mut());
            for (lane, (out, state)) in slots.enumerate() {
                let increment = if held {
                    held_increments[lane]
                } else {
                    phase_increment(pitch[lane], fm[lane], p.fm_mode, sample_rate)
                };
                *out = sanitize(state.step(
                    increment,
                    shape[lane].clamp(0.0, 5.0),
                    offset[lane],
                    sync.map(|sync| sync[lane]),
                    p.wait_for_zero_cross,
                    p.bandlimit,
                ));
            }
        }
        let shape_last = shape.row(last);
        for (lane, state) in self.lane_state[..lanes].iter_mut().enumerate() {
            state
                .shape
                .settle(shape_last[lane].clamp(0.0, 5.0), p.shape.glide_mark(lane));
        }
        Some(())
    }
}

message_handlers!(impl SawOscillator {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly::PolySignal;
    use crate::types::{OutputStruct, Signal};

    fn make_saw(phase_offset: f32, wait_for_zero_cross: bool) -> SawOscillator {
        let params = SawOscillatorParams {
            freq: PolySignal::mono(Signal::Volts(0.0)), // C4 ≈ 261 Hz
            shape: None,                                // 0 → saw
            fm: None,
            fm_mode: FmMode::default(),
            sync: None,
            phase_offset: Some(PolySignal::mono(Signal::Volts(phase_offset))),
            wait_for_zero_cross,
            bandlimit: true,
        };
        let lanes = params.freq.lanes().max(1);
        let mut outputs = SawOscillatorOutputs::default();
        outputs.set_all_lanes(lanes);
        SawOscillator {
            params,
            outputs,
            _lane_count: lanes,
            _channels: 1,
            _block_index: Default::default(),
            lane_state: vec![LaneState::default(); lanes].into_boxed_slice(),
        }
    }

    /// Collect `n` output samples from lane 0.
    fn run(osc: &mut SawOscillator, n: usize) -> Vec<f32> {
        (0..n)
            .map(|_| {
                osc.update(48_000.0);
                osc.outputs.sample.get(0)
            })
            .collect()
    }

    /// Renders 60 samples of an unmuted saw, the sync input rising at
    /// sample 50 when `synced`.
    fn render_synced(bandlimit: bool, synced: bool) -> Vec<f32> {
        let mut osc = make_saw(0.0, false);
        osc.params.bandlimit = bandlimit;
        (0..60)
            .map(|n| {
                let sync = if synced && n >= 50 { 5.0 } else { 0.0 };
                osc.params.sync = Some(PolySignal::mono(Signal::Volts(sync)));
                osc.update(48_000.0);
                osc.outputs.sample.get(0)
            })
            .collect()
    }

    /// Without band-limiting a sync reset is an exact step: the edge sample
    /// still reads the running wave, and the next reads one increment past
    /// phase 0.
    #[test]
    fn unbandlimited_sync_resets_exactly() {
        let free = render_synced(false, false);
        let naive = render_synced(false, true);
        assert_eq!(naive[..51], free[..51]);
        let increment = phase_increment(0.0, 0.0, FmMode::default(), 48_000.0);
        assert_eq!(naive[51], naive_triangle(increment, 1.0) as f32 * 5.0);
        assert_ne!(
            render_synced(true, true)[50],
            render_synced(true, false)[50],
            "the band-limited reset is smoothed"
        );
    }

    #[test]
    fn offset_start_is_muted_until_a_zero_crossing() {
        // A phase offset starts the voice away from a zero crossing. With the
        // mute on, the output is held silent for many samples and then releases
        // on a near-zero sample (no click).
        let mut osc = make_saw(0.25, true);
        let out = run(&mut osc, 400);

        assert_eq!(out[0], 0.0, "first sample should be muted");
        let release = out.iter().position(|&v| v != 0.0).expect("must release");
        assert!(
            release > 1,
            "should hold silence past the first sample, released at {release}"
        );
        assert!(
            out[release].abs() < 0.5,
            "release sample {} should be near zero (click-free)",
            out[release]
        );

        // The wait tracks the offset itself (~0.25 of a cycle), not its
        // complement: starting just before the gentle crossing avoids waiting
        // nearly a whole extra oscillation. C4 ≈ 261.63 Hz → period ≈ 183.5
        // samples, so 0.25 of a cycle ≈ 46 samples.
        let period = 48_000.0 / 261.63;
        let expected = 0.25 * period;
        assert!(
            (release as f32 - expected).abs() < 0.05 * period,
            "release {release} should be ~{expected:.0} samples (offset·period), not its complement"
        );
    }

    #[test]
    fn offset_start_clicks_when_mute_disabled() {
        // With the mute off, the same offset start emits its (non-zero) value
        // immediately — the offset is honored at the cost of a click.
        let mut osc = make_saw(0.25, false);
        let out = run(&mut osc, 4);
        assert!(
            out[0].abs() > 0.5,
            "first sample {} should be non-zero",
            out[0]
        );
    }

    #[test]
    fn zero_offset_starts_immediately_on_the_crossing() {
        // No offset: the seed already lands on a zero crossing, so the mute
        // releases right away even though it is enabled.
        let mut osc = make_saw(0.0, true);
        let out = run(&mut osc, 8);
        let release = out.iter().position(|&v| v != 0.0).expect("must release");
        assert!(
            release <= 1,
            "zero-offset voice should sound immediately, released at {release}"
        );
    }

    #[test]
    fn low_frequency_output_is_a_clean_ramp() {
        // A sub-audio (LFO-rate) saw must be a smooth ramp. The DPW body divides
        // a difference of near-equal integral values by the phase increment;
        // when that increment is tiny (low frequency) the division must keep
        // enough precision that f32 rounding does not blow up into per-sample
        // hash. Regression for the audible high-frequency artifact heard when a
        // 0.2 Hz saw is used as an amplitude LFO.
        let v = (0.2_f32 / 261.625_58).log2(); // 0.2 Hz expressed in V/Oct
        let mut osc = make_saw(0.0, true);
        osc.params.freq = PolySignal::mono(Signal::Volts(v));

        // 0.2 Hz ⇒ ~240k samples/cycle, so 2000 samples stay on the rising
        // segment. Skip the seed/mute-release samples and measure the slope.
        let out = run(&mut osc, 2000);
        let diffs: Vec<f32> = out[50..].windows(2).map(|w| w[1] - w[0]).collect();
        let mean = diffs.iter().sum::<f32>() / diffs.len() as f32;
        let std =
            (diffs.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / diffs.len() as f32).sqrt();

        assert!(
            mean > 0.0,
            "ramp should rise; mean per-sample step = {mean}"
        );
        // The pre-fix f32 DPW produced a step std ~0.04 (≈600x the true slope);
        // a clean ramp has step std at the f32 output-quantization floor (~1e-6).
        assert!(
            std < 1.0e-3,
            "per-sample step std {std} indicates high-frequency hash (clean ramp ≈ 0)"
        );
    }
}
