//! `$wowFlutter`: Beads' dirty-tape wow and flutter at the engine rate.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::rng::{LcgRng, seed_base};
use crate::dsp::utils::sanitize;
use crate::poly::{LaneSum, LaneSumExt, PolyOutput, PolySignal};

use super::wow_flutter::{WowModulation, write_read};
use super::{FULL_SCALE_VOLTS, input_value};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

/// Rate the hardware runs its wow and flutter at.
const HARDWARE_RATE: f32 = 24000.0;
/// Delay swing at `intensity` 5, relative to the hardware's.
const MAX_DEPTH: f32 = 2.0;
/// Longest modulated delay, in samples at [`HARDWARE_RATE`]: the hardware's
/// 1 + 122 at the deepest swing, plus the interpolated read.
const MAX_HARDWARE_DELAY: f32 = 2.0 + 122.0 * MAX_DEPTH;
/// The hardware's low-pass coefficient at [`HARDWARE_RATE`].
const HARDWARE_LP: f32 = 0.95;
/// Control blocks per second.
const BLOCK_RATE: f32 = 4000.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct WowFlutterParams {
    #[input("signal to run through the tape; each lane wobbles in step", type = audio)]
    input: PolySignal,
    #[input(
        "how fast the tape wobbles; each volt doubles it, and 2.5 is Beads' dirty tape",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    speed: Option<LaneSum>,
    #[input(
        "how deep the wobble is: 0 none, 2.5 Beads' dirty tape, 5 twice as deep",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    intensity: Option<LaneSum>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct WowFlutterOutputs {
    #[output("output", "the input with wow and flutter", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

/// One lane's delay line and low-pass.
#[derive(Default)]
struct Line {
    /// Power-of-two length covering the longest delay at the engine rate.
    line: Box<[f32]>,
    write_ptr: usize,
    lp_state: f32,
}

#[derive(Default)]
struct WowFlutterState {
    /// One wobble per side, seeded alike, so a stereo lane's sides wobble
    /// together unless `speed` or `intensity` differ between them.
    modulation: [WowModulation; 2],
    rng: [LcgRng; 2],
    /// Engine samples per hardware sample.
    ratio: f32,
    /// Low-pass coefficient at the engine rate.
    lp: f32,
    block_size: usize,
    block_pos: usize,
}

const WOW_FLUTTER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Tape wow and flutter",
    description: "The drifting pitch wobble and softened highs of worn tape, from Mutable \
                  Instruments Beads' dirty tape setting. Every lane wobbles together, as if \
                  one tape carried them all.",
    categories: &[ModuleCategory::Effect],
    sections: &[],
    examples: &[
        DocExample {
            title: "Warbly tape chords",
            code: "$saw($chord('F3[maj9]')).lpf('f5', 1).wowFlutter().out()",
        },
        DocExample {
            title: "The full Beads chain",
            code: r#"
                const grains = $pulse('8hz')
                const cloud = $granular($saw('a3'), grains, {
                  mix: 4,
                  quality: 'dirtyTape',
                })
                $reverb($wowFlutter(cloud), 3, { size: 5, tone: 2 }).out()
            "#,
        },
    ],
};

#[module(name = "$wowFlutter", docs = WOW_FLUTTER_DOCS, has_init, args(input))]
pub struct WowFlutter {
    outputs: WowFlutterOutputs,
    state: WowFlutterState,
    params: WowFlutterParams,
    lane_state: Box<[Line]>,
}

impl WowFlutter {
    /// Allocate each lane's delay line for `sample_rate`. Main thread.
    fn init(&mut self, sample_rate: f32) {
        let seed = seed_base(self);
        super::primitives::warm_tables();
        let state = &mut self.state;
        state.ratio = sample_rate / HARDWARE_RATE;
        state.lp = 1.0 - (1.0 - HARDWARE_LP).powf(HARDWARE_RATE / sample_rate);
        for (modulation, rng) in state.modulation.iter_mut().zip(&mut state.rng) {
            modulation.init(sample_rate);
            rng.seed(seed, 0);
        }
        state.block_size = ((sample_rate / BLOCK_RATE) as usize).max(1);
        state.block_pos = 0;
        let len = ((MAX_HARDWARE_DELAY * state.ratio) as usize + 2).next_power_of_two();
        for line in self.lane_state.iter_mut() {
            line.line = vec![0.0; len].into_boxed_slice();
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channels();
        let state = &mut self.state;
        let mut delays = [0.0; 2];
        for (side, delay) in delays.iter_mut().enumerate().take(channels) {
            if state.block_pos == 0 {
                let p = &self.params;
                let control = |s: &Option<LaneSum>| sanitize(s.side_or(side, 2.5)).clamp(0.0, 5.0);
                let speed = (control(&p.speed) - 2.5).exp2();
                let depth = control(&p.intensity) / 5.0 * MAX_DEPTH;
                state.modulation[side].start_block(
                    state.block_size,
                    &mut state.rng[side],
                    speed,
                    depth,
                );
            }
            *delay = state.modulation[side].next() * state.ratio;
        }
        state.block_pos = (state.block_pos + 1) % state.block_size;
        for (lane, line) in self.lane_state.iter_mut().enumerate() {
            let x = input_value(&self.params.input, lane);
            let delay = delays[lane % channels];
            let y = write_read(&mut line.line, &mut line.write_ptr, x, delay);
            line.lp_state += state.lp * (y - line.lp_state);
            self.outputs
                .sample
                .set(lane, line.lp_state * FULL_SCALE_VOLTS);
        }
    }
}

message_handlers!(impl WowFlutter {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, Signal};

    const SR: f32 = 48000.0;

    fn make(lanes: usize, sample_rate: f32) -> WowFlutter {
        let mut outputs = WowFlutterOutputs::default();
        outputs.set_all_lanes(lanes);
        let mut m = WowFlutter {
            outputs,
            state: Default::default(),
            params: WowFlutterParams {
                input: PolySignal::poly(&vec![Signal::Volts(0.0); lanes]),
                speed: None,
                intensity: None,
            },
            lane_state: (0..lanes).map(|_| Line::default()).collect(),
            _lane_count: lanes,
            _channels: 1,
            _block_index: Default::default(),
        };
        m.init(sample_rate);
        m
    }

    /// Feed each lane's signal from `input` for `n` samples and return
    /// every lane's output.
    fn run(m: &mut WowFlutter, n: usize, input: impl Fn(usize, usize) -> f32) -> Vec<Vec<f32>> {
        let lanes = m.lane_state.len();
        let mut out = vec![Vec::new(); lanes];
        for i in 0..n {
            let signals: Vec<Signal> = (0..lanes).map(|c| Signal::Volts(input(i, c))).collect();
            m.params.input = PolySignal::poly(&signals);
            m.update(SR);
            for (c, o) in out.iter_mut().enumerate() {
                o.push(m.outputs.sample.get(c));
            }
        }
        out
    }

    #[test]
    fn passes_a_steady_level_unchanged() {
        let mut m = make(1, SR);
        let out = run(&mut m, 48_000, |_, _| 2.0);
        assert!((out[0][47_999] - 2.0).abs() < 1e-4, "{}", out[0][47_999]);
    }

    #[test]
    fn lanes_share_one_modulation() {
        let mut m = make(3, SR);
        let out = run(&mut m, 48_000, |i, _| (i as f32 * 0.05).sin());
        assert_eq!(out[0], out[1]);
        assert_eq!(out[0], out[2]);
    }

    /// Positive-going zero crossings of `xs`, interpolated.
    fn crossings(xs: &[f32]) -> Vec<f64> {
        (1..xs.len())
            .filter(|&i| xs[i - 1] < 0.0 && xs[i] >= 0.0)
            .map(|i| (i - 1) as f64 + xs[i - 1] as f64 / (xs[i - 1] - xs[i]) as f64)
            .collect()
    }

    fn volts(v: f32) -> Option<LaneSum> {
        Some(LaneSum::from_poly(PolySignal::mono(Signal::Volts(v))))
    }

    /// Shortest and longest period of a 1 kHz tone through `m`, in samples.
    fn period_range(m: &mut WowFlutter) -> (f64, f64) {
        let out = run(m, 4 * 48_000, |i, _| {
            (std::f32::consts::TAU * 1000.0 * i as f32 / SR).sin()
        });
        let c = crossings(&out[0][48_000..]);
        c.windows(2)
            .map(|w| w[1] - w[0])
            .fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)))
    }

    #[test]
    fn wobbles_the_pitch() {
        let (min, max) = period_range(&mut make(1, SR));
        // A steady 1 kHz tone is 48 samples per cycle; the wobble stretches
        // and squeezes it.
        assert!(max - min > 0.1, "periods {min}..{max}");
        assert!((min - 48.0).abs() < 3.0 && (max - 48.0).abs() < 3.0);
    }

    #[test]
    fn no_intensity_leaves_the_pitch_steady() {
        let mut m = make(1, SR);
        m.params.intensity = volts(0.0);
        let (min, max) = period_range(&mut m);
        // Within the accuracy of the zero-crossing estimate.
        assert!(max - min < 0.05, "periods {min}..{max}");
    }

    #[test]
    fn speed_and_intensity_deepen_the_wobble() {
        let swing = |speed: f32, intensity: f32| {
            let mut m = make(1, SR);
            m.params.speed = volts(speed);
            m.params.intensity = volts(intensity);
            let (min, max) = period_range(&mut m);
            max - min
        };
        let beads = swing(2.5, 2.5);
        assert!(swing(2.5, 5.0) > 1.5 * beads, "deeper");
        assert!(swing(4.5, 2.5) > 2.0 * beads, "faster");
        assert!(swing(0.5, 2.5) < 0.5 * beads, "slower");
    }

    #[test]
    fn the_deepest_fastest_wobble_stays_finite() {
        let mut m = make(1, SR);
        m.params.speed = volts(5.0);
        m.params.intensity = volts(5.0);
        let out = run(&mut m, 4 * 48_000, |i, _| {
            (std::f32::consts::TAU * 1000.0 * i as f32 / SR).sin()
        });
        assert!(out[0].iter().all(|x| x.is_finite() && x.abs() < 5.0));
    }

    #[test]
    fn delay_line_covers_the_longest_delay_at_high_rates() {
        let m = make(1, 192_000.0);
        assert!(m.lane_state[0].line.len() as f32 > MAX_HARDWARE_DELAY * 8.0 + 1.0);
    }
}
