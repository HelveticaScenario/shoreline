//! The North Coast Synthesis MSK 007 Leapfrog: five integrators in a row,
//! each integrating the difference between its two neighbours, whose
//! outputs a fixed mixer sums into a near-elliptic fifth-order lowpass.
//!
//! The filter's own response has no resonance. A VCA feeds the mixer's
//! output back to the input buffer, which inverts it, through a Zener
//! clipper, and the loop self-oscillates at the frequency where the
//! response turns the phase 180°. Each sample solves the trapezoidal
//! integrators and that loop together, with the clipper's gain linearised
//! around the previous sample's output, so the feedback path carries no
//! delay.

use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;

use super::kernel::{DB24, Kernel, Rates, Taps, loop_gain};
use super::runner::{AnalogLane, AnalogPorts, AnalogState, CoeffKey, Inputs, run};
use crate::dsp::filters::resonant_svf::FilterParams;
use crate::dsp::utils::sanitize;
use crate::poly::{PolyOutput, PolySignal};
use crate::types::{OutputStruct, PatchUpdateHandler};

/// Each integrator's time constant, in seconds at a cutoff of 1 rad/s. The
/// cutoff is the last peak of the passband; the nulls sit at 2 and 3 times
/// it.
const TAU: [f32; 5] = [
    1.301_466_6,
    1.510_573_6,
    2.319_052_3,
    1.267_479_2,
    1.398_601_4,
];

/// The output mixer's weight on each integrator.
const ALPHA: [f32; 5] = [
    0.004_474_30,
    -0.004_831_24,
    0.193_072_99,
    -0.171_015_98,
    0.978_299_9,
];

/// The feedback gain at which the loop self-oscillates: the inverse of the
/// response's level where its phase reaches 180°, at 0.73 of the cutoff.
const SELF_OSCILLATION: f32 = 1.0 / 0.956_850_6;

/// Volts at which the input buffer's, an integrator's or the mixer's op amp
/// reaches its supply.
const RAIL: f32 = 10.5;

/// The feedback clipper: above `SOFT_CLIP` volts a 4.7 V Zener pair loads
/// the VCA's output through 39 kΩ beside the 100 kΩ it drives, and a 6.0 V
/// pair holds it at `HARD_CLIP`.
const SOFT_CLIP: f32 = 5.3;
const HARD_CLIP: f32 = 6.6;
const SOFT_SLOPE: f32 = 39.0 / 139.0;

/// The clipper's output over its input `w`, in volts.
#[inline]
fn clip_ratio(w: f32) -> f32 {
    let level = w.abs();
    if level <= SOFT_CLIP {
        1.0
    } else {
        (SOFT_CLIP + SOFT_SLOPE * (level - SOFT_CLIP)).min(HARD_CLIP) / level
    }
}

#[derive(Clone, Copy, Default)]
pub struct LeapfrogCoeffs {
    /// Each integrator's gain.
    g: [f32; 5],
    /// The tridiagonal solve's pivots and upper factors.
    inv: [f32; 5],
    upper: [f32; 4],
    /// Each integrator's voltage, and the mixer's, per volt at the core's
    /// input.
    q: [f32; 5],
    yq: f32,
    /// Feedback gain.
    k: f32,
}

impl LeapfrogCoeffs {
    /// The integrator voltages for the right-hand side `rhs`.
    #[inline]
    fn solve(&self, rhs: [f32; 5]) -> [f32; 5] {
        let mut v = rhs;
        v[0] *= self.inv[0];
        for i in 1..5 {
            v[i] = (v[i] + self.g[i] * v[i - 1]) * self.inv[i];
        }
        for i in (0..4).rev() {
            v[i] -= self.upper[i] * v[i + 1];
        }
        v
    }
}

#[inline]
fn mix(v: &[f32; 5]) -> f32 {
    ALPHA.iter().zip(v).map(|(a, v)| a * v).sum()
}

#[derive(Clone, Copy, Default)]
pub struct LeapfrogState {
    /// Trapezoidal integrator states.
    s: [f32; 5],
    /// The previous sample's mixer output.
    y: f32,
}

/// The five-integrator leapfrog core and its feedback loop, in volts.
#[derive(Clone, Copy, Default)]
pub struct Leapfrog;

impl Kernel for Leapfrog {
    type Response = ();
    type Options = ();
    type Coeffs = LeapfrogCoeffs;
    type State = LeapfrogState;
    const VOLTS: f32 = 1.0;

    fn coeffs(_: (), cutoff: f32, resonance: f32, rates: Rates, _: Taps, _: ()) -> LeapfrogCoeffs {
        let gain = rates.gain(cutoff);
        let g = TAU.map(|tau| gain / tau);
        // Integrator `i` solves `v[i] = s[i] + g[i]·(v[i−1] − v[i+1])`; the
        // first reads the core's input for `v[i−1]` and the last its own
        // output for `v[i+1]`.
        let mut inv = [1.0; 5];
        let mut upper = [0.0; 4];
        for i in 0..5 {
            let diagonal = if i == 4 { 1.0 + g[4] } else { 1.0 };
            let below = if i == 0 { 0.0 } else { g[i] * upper[i - 1] };
            inv[i] = 1.0 / (diagonal + below);
            if i < 4 {
                upper[i] = g[i] * inv[i];
            }
        }
        let mut c = LeapfrogCoeffs {
            g,
            inv,
            upper,
            ..Default::default()
        };
        c.q = c.solve([g[0], 0.0, 0.0, 0.0, 0.0]);
        c.yq = mix(&c.q);
        c.k = loop_gain(resonance, SELF_OSCILLATION);
        c
    }

    #[inline]
    fn step(_: (), c: &LeapfrogCoeffs, state: &mut LeapfrogState, x: f32, taps: Taps) -> [f32; 4] {
        let mut v = c.solve(state.s);
        // The core's input `u = x − k·y`, with `y` the mix of `v + q·u`.
        let k = c.k * clip_ratio(c.k * state.y);
        let u = ((x - k * mix(&v)) / (1.0 + k * c.yq)).clamp(-RAIL, RAIL);
        for i in 0..5 {
            let ideal = v[i] + c.q[i] * u;
            v[i] = ideal.clamp(-RAIL, RAIL);
            // An integrator at its rail keeps integrating from there.
            state.s[i] = sanitize(v[i] + ideal - state.s[i]);
        }
        let y = sanitize(mix(&v).clamp(-RAIL, RAIL));
        state.y = y;
        let mut out = [0.0; 4];
        if taps[DB24] {
            out[DB24] = y;
        }
        out
    }
}

const LPF_LEAPFROG_DOCS: ModuleDocs = ModuleDocs {
    summary: "Leapfrog brick-wall lowpass filter",
    description: "The North Coast Synthesis Leapfrog's five-pole near-elliptic lowpass: almost \
                  a brick wall above the cutoff, with nulls at twice and three times the cutoff \
                  and a rippling passband that rings on sharp edges. Resonance feeds the output \
                  back through a clipper until the filter sings, about a fourth below the \
                  cutoff.",
    categories: &[ReferenceCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Brick-wall sweep",
            code: r#"
                chord: $saw('c2:m7')
                  .lpfLeapfrog($sine('0.1hz').range('c4', 'c7'))
                  .gain(4.5)
            "#,
        },
        DocExample {
            title: "Ringing bass",
            code: r#"
                const seq = $`c2 c2 [eb2 c3] g1`
                bass: $pulse(seq).lpfLeapfrog(
                  $perc(seq, { decay: 0.25 }).range('c3', 'c6'),
                  3.5,
                )
            "#,
        },
    ],
};

#[params]
#[derive(Clone, deserr::Deserr, schemars::JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct LpfLeapfrogParams {
    #[input("the sound to filter", type = audio)]
    input: PolySignal,
    #[input(
        "cutoff frequency: the last peak before the wall; tops out just under half the sample rate",
        type = frequency,
        range = (-3.71, ..),
    )]
    cutoff: PolySignal,
    #[input(
        "feedback around the filter: a peak rises below the cutoff as the level under it falls; self-oscillates from about 4.5",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    resonance: Option<PolySignal>,
    #[input(
        "run at twice the engine rate: less aliasing from clipping and self-oscillation, and truer nulls at high cutoffs; false costs less CPU"
    )]
    #[serde(default = "crate::dsp::utils::default_true")]
    #[deserr(default = crate::dsp::utils::default_true())]
    high_res: bool,
}

#[derive(Outputs, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct LpfLeapfrogOutputs {
    #[output("output", "5-pole near-elliptic lowpass", type = cv, default)]
    output: PolyOutput,
}

impl AnalogPorts for LpfLeapfrogOutputs {
    fn taps(connected: &<Self as OutputStruct>::Connected) -> Taps {
        let mut taps = [false; 4];
        taps[DB24] = connected.output;
        taps
    }

    fn ports(&mut self) -> [Option<&mut PolyOutput>; 4] {
        let mut ports = [None, None, None, None];
        ports[DB24] = Some(&mut self.output);
        ports
    }
}

#[module(
    name = "$lpfLeapfrog",
    docs = LPF_LEAPFROG_DOCS,
    args(input, cutoff, resonance),
    has_init,
    patch_update,
    experimental
)]
pub struct LpfLeapfrog {
    outputs: LpfLeapfrogOutputs,
    state: AnalogState<Leapfrog>,
    lane_state: Box<[AnalogLane<Leapfrog>]>,
    params: LpfLeapfrogParams,
}

impl LpfLeapfrog {
    fn init(&mut self, _sample_rate: f32) {
        let base = crate::dsp::utils::rng::seed_base(self);
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            state.seed(base, lane);
        }
    }

    /// The connected output and `highRes`.
    fn key(&self) -> CoeffKey<Leapfrog> {
        (
            LpfLeapfrogOutputs::taps(&self.connected()),
            self.params.high_res,
            (),
        )
    }

    fn update(&mut self, sample_rate: f32) {
        let lanes = self.lane_count();
        let key = self.key();
        run::<Leapfrog>(
            (),
            lanes,
            &Inputs {
                input: &self.params.input,
                filter: FilterParams {
                    cutoff: &self.params.cutoff,
                    resonance: &self.params.resonance,
                    default_resonance: 0.0,
                },
                drive: &None,
            },
            key,
            sample_rate,
            &mut self.state,
            &mut self.lane_state,
            self.outputs.ports(),
        );
    }
}

impl PatchUpdateHandler for LpfLeapfrog {
    fn on_patch_update(&mut self) {
        let key = self.key();
        self.state.on_patch_update(
            key,
            &self.params.cutoff,
            &self.params.resonance,
            &mut self.lane_state,
        );
    }
}

message_handlers!(impl LpfLeapfrog {});
