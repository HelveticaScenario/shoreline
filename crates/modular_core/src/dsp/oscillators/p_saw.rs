use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        oscillators::waveforms::{triangle, triangle_symmetry},
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
struct PSawOscillatorParams {
    #[input(
        "phase to read the wave at, usually from `$ramp`: the range is one cycle; values outside wrap",
        type = audio,
        range = (0.0, 1.0),
    )]
    phase: PolySignal,
    #[input(
        "waveform: 0 saw, 2.5 triangle, 5 ramp, morphing in between",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    shape: Option<PolySignal>,
    #[input(
        "smooths the edges so high notes stay clean; off gives exact steps and corners, for LFOs or a raw aliased tone"
    )]
    #[serde(default = "default_true")]
    #[deserr(default = default_true())]
    bandlimit: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PSawOscillatorOutputs {
    #[output("output", "the saw/triangle/ramp wave", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

/// Per-lane state for shape glide and phase tracking
#[derive(Default, Clone, Copy, SeedSide)]
struct LaneState {
    shape: SwapGlide,
    /// Previous phase, kept in f64 so the DPW differencing (which divides a
    /// difference of near-equal integral values by the per-sample phase delta)
    /// stays well-conditioned when that delta is tiny at slow phasor rates.
    prev_phase: f64,
}

const P_SAW_DOCS: ModuleDocs = ModuleDocs {
    summary: "Phase-driven saw and triangle",
    description: "The saw-to-triangle morph of `$saw`, following a phase input \
                  instead of its own pitch. Warp the phase with `$pWarp` for \
                  sync sweeps, folds and other reshaped cycles.",
    categories: &[ModuleCategory::Oscillator, ModuleCategory::Phase],
    sections: &[],
    examples: &[DocExample {
        title: "Hard-sync sweep",
        code: r#"
            tone: $pSaw(
              $pWarp(
                $ramp('c2'),
                $phase.sync($sine('0.2hz').range(0, 3)),
                { freq: 'c2' },
              ),
            ).lpf('c6')
        "#,
    }],
};

#[module(name = "$pSaw", docs = P_SAW_DOCS, args(phase))]
pub struct PSawOscillator {
    outputs: PSawOscillatorOutputs,
    lane_state: Box<[LaneState]>,
    params: PSawOscillatorParams,
}

impl PSawOscillator {
    fn update(&mut self, sample_rate: f32) {
        let num_lanes = self.lane_count();

        for lane in 0..num_lanes {
            let state = &mut self.lane_state[lane];

            let shape_val = self.params.shape.value_or(lane, 0.0).clamp(0.0, 5.0);
            state
                .shape
                .update(shape_val, self.params.shape.glide_mark(lane), sample_rate);

            let phase = wrap(0.0..1.0, self.params.phase.get_value(lane)) as f64;

            // Calculate phase increment from phase difference
            // Handle phase wrapping correctly
            let mut phase_increment = phase - state.prev_phase;
            if phase_increment < -0.5 {
                phase_increment += 1.0;
            } else if phase_increment > 0.5 {
                phase_increment -= 1.0;
            }
            phase_increment = phase_increment.abs().clamp(0.0, 0.5);

            let s = triangle_symmetry(*state.shape, self.params.bandlimit);
            let raw_output = triangle(
                state.prev_phase,
                phase,
                phase_increment,
                s,
                self.params.bandlimit,
            );

            state.prev_phase = phase;
            self.outputs.sample.set(lane, raw_output as f32 * 5.0);
        }
    }
}

message_handlers!(impl PSawOscillator {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::oscillators::waveforms::naive_triangle;
    use crate::poly::PolySignal;
    use crate::types::{OutputStruct, Signal};

    fn make_p_saw(bandlimit: bool) -> PSawOscillator {
        let params = PSawOscillatorParams {
            phase: PolySignal::mono(Signal::Volts(0.0)),
            shape: None, // 0 → saw
            bandlimit,
        };
        let lanes = params.phase.lanes().max(1);
        let mut outputs = PSawOscillatorOutputs::default();
        outputs.set_all_lanes(lanes);
        PSawOscillator {
            params,
            outputs,
            _lane_count: lanes,
            _channels: 1,
            _block_index: Default::default(),
            lane_state: vec![LaneState::default(); lanes].into_boxed_slice(),
        }
    }

    /// Without band-limiting the output is the exact wave at the input phase.
    #[test]
    fn unbandlimited_reads_the_exact_wave() {
        let mut osc = make_p_saw(false);
        for i in 0..200 {
            let phase = (i as f32 * 0.037).fract();
            osc.params.phase = PolySignal::mono(Signal::Volts(phase));
            osc.update(48_000.0);
            let expected = naive_triangle(phase as f64, triangle_symmetry(0.0, false)) as f32 * 5.0;
            assert_eq!(osc.outputs.sample.get(0), expected);
        }
    }

    #[test]
    fn low_rate_phase_ramp_is_a_clean_ramp() {
        // Driving $pSaw with a slow external phasor (LFO rate) must yield a
        // smooth ramp. The DPW body divides a difference of near-equal integral
        // values by the per-sample phase delta; when that delta is tiny the
        // differencing must keep enough precision that f32 rounding does not
        // blow up into per-sample hash. Mirrors the saw.rs regression.
        let mut osc = make_p_saw(true);

        // 0.2 Hz at 48 kHz ⇒ the phasor advances 0.2/48000 per sample. Start
        // mid-ramp (0.5) so 2000 samples stay on the rising segment (no wrap).
        let inc = 0.2_f32 / 48_000.0;
        let mut out = Vec::with_capacity(2000);
        for i in 0..2000 {
            let ph = 0.5 + inc * i as f32;
            osc.params.phase = PolySignal::mono(Signal::Volts(ph));
            osc.update(48_000.0);
            out.push(osc.outputs.sample.get(0));
        }

        // Skip the first samples (the initial step from prev_phase = 0), then
        // measure the slope of the ramp.
        let diffs: Vec<f32> = out[10..].windows(2).map(|w| w[1] - w[0]).collect();
        let mean = diffs.iter().sum::<f32>() / diffs.len() as f32;
        let std =
            (diffs.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / diffs.len() as f32).sqrt();

        assert!(
            mean > 0.0,
            "ramp should rise; mean per-sample step = {mean}"
        );
        // Pre-fix f32 DPW produced step std ~0.03 here; a clean ramp sits at the
        // f32 output-quantization floor (~1e-8).
        assert!(
            std < 1.0e-3,
            "per-sample step std {std} indicates high-frequency hash (clean ramp ≈ 0)"
        );
    }
}
