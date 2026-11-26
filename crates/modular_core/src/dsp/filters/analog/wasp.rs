//! The EDP Wasp's filter: a state-variable loop whose three amplifiers are
//! unbuffered CMOS inverters on a 5 V rail, a summer and two integrators
//! each fed by an OTA.
//!
//! The circuit is solved as it is wired. Each inverter is a complementary
//! pair of square-law MOSFETs, so its gain is low, its input node wanders
//! with its output, and it runs out of drive toward either rail. The
//! integrating capacitors sit between an inverter's input and output. The
//! Q pot feeds the bandpass back to the summer, and beside it a pair of
//! diodes adds damping as the bandpass swings further, which holds the
//! resonance on the verge of oscillation. The diodes rest slightly biased,
//! since the inverters' threshold sits above half the rail, and that bias
//! sets the highest Q the loop reaches.
//!
//! Voltages are deviations from the circuit at rest, in volts; currents
//! are in mA and conductances in mS. Each sample solves the five node
//! equations with Newton's method from the previous sample's voltages.

use super::kernel::{DB12, Kernel, Rates, Taps};

/// The inverters' supply.
const VDD: f64 = 5.0;
/// The inverters' input and the summer's and bandpass's outputs at rest:
/// the threshold that puts the lowpass output at its typical 3 V.
const THRESHOLD: f64 = 2.667;
const MID_RAIL: f64 = 0.5 * VDD;
const LOWPASS_REST: f64 = 3.0;
/// Gate thresholds of the N and P devices, which set [`THRESHOLD`].
const VT_N: f64 = 1.5 + (THRESHOLD - MID_RAIL);
const VT_P: f64 = 1.5 - (THRESHOLD - MID_RAIL);
/// Transconductance parameter in mA/V², and channel-length modulation per
/// volt: 1 mA of output drive at 5 V and an open-loop gain near 30.
const BETA: f64 = 0.8;
const LAMBDA: f64 = 0.065;

/// Summer resistors R29, R30 and R31, with the input resistor their equal
/// for unity gain.
const G_SUM: f64 = 1.0 / 33.0;
/// The Q pot VR6.
const R_POT: f64 = 100.0;
/// Rotation the pot's end resistance leaves at full resonance.
const POT_END: f64 = 0.01;
/// R27 and R28, from the bandpass to the diodes and on to AC ground.
const G_DIODE_FEED: f64 = 1.0 / 100.0;
/// An OTA's input network as its inverter's load: 100 kΩ into 1 kΩ and the
/// 33 kΩ bias pair.
const G_OTA_INPUT: f64 = 1.0 / 117.5;
/// `tanh` argument per volt at an OTA's input network.
const OTA_SCALE: f64 = 1.0 / (117.5 * 2.0 * 0.025_85);
/// Limits of an integrator's input node, where its OTA's output runs out
/// of swing.
const NODE_MIN: f64 = 0.6 - THRESHOLD;
const NODE_MAX: f64 = 3.5 - THRESHOLD;

/// 1N4148 saturation current in mA and emission coefficient × thermal
/// voltage.
const DIODE_IS: f64 = 2.52e-6;
const DIODE_NVT: f64 = 1.752 * 0.025_85;
/// The diodes' bias at rest.
const DIODE_REST: f64 = MID_RAIL - THRESHOLD;
/// Most a Newton step moves the diode voltage.
const DIODE_STEP: f64 = 0.1;

/// Integrator capacitors C12 and C13 (1 nF), and C11 across the lowpass
/// feedback (68 pF), in mF so that one times a rate in Hz is in mS.
const C_INTEGRATOR: f64 = 1.0e-6;
const C_LEAD: f64 = 68.0e-9;

const MAX_ITERATIONS: usize = 8;
/// Volts a Newton step must fall under to end the solve.
const TOLERANCE: f64 = 1.0e-5;

/// Drain current of a MOSFET and its slopes in `vgs` and `vds`.
const fn mosfet(vgs: f64, vds: f64, vt: f64) -> (f64, f64, f64) {
    let vov = vgs - vt;
    if vov <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let modulation = 1.0 + LAMBDA * vds;
    if vds < vov {
        let a = vov * vds - 0.5 * vds * vds;
        (
            BETA * a * modulation,
            BETA * vds * modulation,
            BETA * ((vov - vds) * modulation + a * LAMBDA),
        )
    } else {
        let a = 0.5 * vov * vov;
        (
            BETA * a * modulation,
            BETA * vov * modulation,
            BETA * a * LAMBDA,
        )
    }
}

/// Current an inverter sources into its output at absolute input and
/// output voltages, and its slopes in each.
const fn inverter(vin: f64, vout: f64) -> (f64, f64, f64) {
    let (i_n, gm_n, go_n) = mosfet(vin, vout, VT_N);
    let (i_p, gm_p, go_p) = mosfet(VDD - vin, VDD - vout, VT_P);
    (i_p - i_n, -gm_p - gm_n, -go_p - go_n)
}

/// One of the three inverters, resting with its output at `rest`.
#[derive(Clone, Copy)]
struct Stage {
    rest: f64,
    /// Current it sources at rest.
    idle: f64,
}

impl Stage {
    const fn new(rest: f64) -> Self {
        Self {
            rest,
            idle: inverter(THRESHOLD, rest).0,
        }
    }

    /// Current sourced beyond the rest current for input and output
    /// deviations `n` and `o`, and its slopes in each.
    #[inline]
    fn current(&self, n: f64, o: f64) -> (f64, f64, f64) {
        let (i, di_dn, di_do) = inverter(THRESHOLD + n, self.rest + o);
        (i - self.idle, di_dn, di_do)
    }

    /// Small-signal transconductance and output conductance at rest.
    const fn conductances(&self) -> (f64, f64) {
        let (_, gm, go) = inverter(THRESHOLD, self.rest);
        (-gm, -go)
    }
}

const SUMMER: Stage = Stage::new(MID_RAIL);
const BANDPASS: Stage = Stage::new(MID_RAIL);
const LOWPASS: Stage = Stage::new(LOWPASS_REST);

/// How far an integrator's low gain scales its output: `1 / (1 + load / gm)`.
const fn integrator_gain(stage: &Stage, load: f64) -> f64 {
    let (gm, go) = stage.conductances();
    1.0 / (1.0 + (go + load) / gm)
}

/// Loads on the bandpass inverter beside the diode feed: the second OTA's
/// input and the Q pot.
const G_BANDPASS_LOAD: f64 = G_OTA_INPUT + 1.0 / R_POT;
/// The two integrators' gains together.
const INTEGRATOR_GAINS: f64 = integrator_gain(&BANDPASS, G_BANDPASS_LOAD + 0.5 * G_DIODE_FEED)
    * integrator_gain(&LOWPASS, G_SUM);

/// The diode pair's current beyond its rest current at a deviation `v`
/// from its rest bias, and its slope.
#[inline]
fn diodes(v: f64) -> (f64, f64) {
    /// What the pair carries at [`DIODE_REST`].
    const REST_CURRENT: f64 = -1.005_892_310_166_743_7e-4;
    let e = ((v + DIODE_REST) / DIODE_NVT).exp();
    let inverse = 1.0 / e;
    (
        DIODE_IS * (e - inverse) - REST_CURRENT,
        DIODE_IS * (e + inverse) / DIODE_NVT,
    )
}

/// An OTA's `tanh` of the voltage at its input network, and the slope of
/// that `tanh` in its argument.
#[inline]
fn ota(volts: f64) -> (f64, f64) {
    let x = OTA_SCALE * volts;
    let x2 = x * x;
    if x2 < 9.0 {
        let root = (9.0 - x2) / (9.0 + 3.0 * x2);
        (x * (27.0 + x2) / (27.0 + 9.0 * x2), root * root)
    } else {
        (x.signum(), 0.0)
    }
}

/// The unknowns: the summer's input node, the three inverter outputs and
/// the voltage across the diodes.
const N1: usize = 0;
const HP: usize = 1;
const BP: usize = 2;
const LP: usize = 3;
const VD: usize = 4;
const UNKNOWNS: usize = 5;

/// Solves `a · x = b` for the node equations' Jacobian, by eliminating the
/// diode voltage, the highpass and the bandpass in turn. Their pivots are
/// each a sum of conductances out of one node, so none can vanish.
#[inline]
fn solve(a: &[[f64; UNKNOWNS]; UNKNOWNS], b: &[f64; UNKNOWNS]) -> [f64; UNKNOWNS] {
    // The diode voltage out of the summer's and the bandpass's equations.
    let diode = 1.0 / a[0][VD];
    let (f1, f3) = (a[1][VD] * diode, a[3][VD] * diode);
    let (s_n1, s_bp, s_b) = (
        a[1][N1] - f1 * a[0][N1],
        a[1][BP] - f1 * a[0][BP],
        b[1] - f1 * b[0],
    );
    let (b_n1, b_bp, b_b) = (
        a[3][N1] - f3 * a[0][N1],
        a[3][BP] - f3 * a[0][BP],
        b[3] - f3 * b[0],
    );
    // The highpass, from its own inverter's equation.
    let highpass = 1.0 / a[2][HP];
    let (h_n1, h_b) = (a[2][N1] * highpass, b[2] * highpass);
    let (s_n1, s_b) = (s_n1 - a[1][HP] * h_n1, s_b - a[1][HP] * h_b);
    let (b_n1, b_b) = (b_n1 - a[3][HP] * h_n1, b_b - a[3][HP] * h_b);
    // The bandpass, leaving the summer's and the lowpass's equations in
    // the summer's input node and the lowpass.
    let bandpass = 1.0 / b_bp;
    let (p_n1, p_b) = (b_n1 * bandpass, b_b * bandpass);
    let (s_n1, s_b) = (s_n1 - s_bp * p_n1, s_b - s_bp * p_b);
    let (l_n1, l_b) = (a[4][N1] - a[4][BP] * p_n1, b[4] - a[4][BP] * p_b);
    let (s_lp, l_lp) = (a[1][LP], a[4][LP]);
    let determinant = 1.0 / (s_n1 * l_lp - s_lp * l_n1);
    let n1 = (s_b * l_lp - s_lp * l_b) * determinant;
    let lp = (s_n1 * l_b - s_b * l_n1) * determinant;
    let bp = p_b - p_n1 * n1;
    let hp = h_b - h_n1 * n1;
    let vd = (b[0] - a[0][N1] * n1 - a[0][BP] * bp) * diode;
    [n1, hp, bp, lp, vd]
}

/// What a Wasp filter module mixes from the loop's three outputs.
#[derive(Clone, Copy)]
pub(crate) enum WaspResponse {
    Lowpass,
    Highpass,
    Bandpass,
    /// The highpass and lowpass summed.
    Notch,
    /// The notch less the bandpass at the weight the Q pot feeds it back.
    Allpass,
}

#[derive(Clone, Copy, Default)]
pub struct WaspCoeffs {
    /// Integrator gain per unit of an OTA's `tanh`, in volts.
    step: f64,
    /// Integrator gain as a slope in the OTA's input voltage.
    g: f64,
    /// An OTA's bias current: what it drives into its inverter per unit of
    /// its `tanh`.
    bias: f64,
    /// C11 as a conductance over one sample.
    lead: f64,
    /// The Q pot's wiper to the summer, and that times the pot's rotation.
    wiper: f64,
    wiper_bandpass: f64,
    /// Brings the highpass to the level of the lowpass, which the summer's
    /// low gain leaves it short of.
    highpass_gain: f64,
    /// The bandpass's weight in the summer beside the lowpass's.
    damping: f64,
}

#[derive(Clone, Copy, Default)]
pub struct WaspState {
    /// The previous sample's solution.
    v: [f64; UNKNOWNS],
    /// Voltage across each integrator's capacitor, input side positive.
    q: [f64; 2],
    /// The previous sample's OTA `tanh`s.
    drive: [f64; 2],
    /// The previous sample's voltage across C11.
    lead: f64,
}

/// The node equations' residuals and their Jacobian at `v`, for the input
/// `x` and the capacitors' history in `state`.
#[inline]
fn equations(
    c: &WaspCoeffs,
    state: &WaspState,
    x: f64,
    v: &[f64; UNKNOWNS],
) -> ([f64; UNKNOWNS], [[f64; UNKNOWNS]; UNKNOWNS]) {
    let (n1, hp, bp, lp, vd) = (v[N1], v[HP], v[BP], v[LP], v[VD]);
    let (diode, diode_slope) = diodes(vd);
    let (drive_hp, slope_hp) = ota(hp);
    let (drive_bp, slope_bp) = ota(bp);
    let q_bp = state.q[0] - c.step * (drive_hp + state.drive[0]);
    let q_lp = state.q[1] - c.step * (drive_bp + state.drive[1]);
    let (i_hp, hp_dn, hp_do) = SUMMER.current(n1, hp);
    let (i_bp, bp_dn, bp_do) = BANDPASS.current(bp + q_bp, bp);
    let (i_lp, lp_dn, lp_do) = LOWPASS.current(lp + q_lp, lp);
    // The node between R27, R28 and the diodes.
    let nd = n1 + vd;
    let lead = c.lead * (lp - n1 - state.lead);
    let residual = [
        G_DIODE_FEED * (bp - nd) - G_DIODE_FEED * nd - diode,
        G_SUM * (x - n1) + G_SUM * (hp - n1) + G_SUM * (lp - n1) + lead + c.wiper_bandpass * bp
            - c.wiper * n1
            + diode,
        i_hp - G_SUM * (hp - n1) - G_OTA_INPUT * hp,
        i_bp - c.bias * drive_hp - G_BANDPASS_LOAD * bp - G_DIODE_FEED * (bp - nd),
        i_lp - c.bias * drive_bp - G_SUM * (lp - n1) - lead,
    ];
    // An OTA's reach into its integrator: the capacitor's voltage moves the
    // inverter's input, and its bias current loads the output.
    let ota_bp = -(bp_dn * c.g + c.bias * OTA_SCALE) * slope_hp;
    let ota_lp = -(lp_dn * c.g + c.bias * OTA_SCALE) * slope_bp;
    let mut jacobian = [[0.0; UNKNOWNS]; UNKNOWNS];
    jacobian[0][N1] = -2.0 * G_DIODE_FEED;
    jacobian[0][BP] = G_DIODE_FEED;
    jacobian[0][VD] = -2.0 * G_DIODE_FEED - diode_slope;
    jacobian[1][N1] = -(3.0 * G_SUM + c.lead + c.wiper);
    jacobian[1][HP] = G_SUM;
    jacobian[1][BP] = c.wiper_bandpass;
    jacobian[1][LP] = G_SUM + c.lead;
    jacobian[1][VD] = diode_slope;
    jacobian[2][N1] = hp_dn + G_SUM;
    jacobian[2][HP] = hp_do - G_SUM - G_OTA_INPUT;
    jacobian[3][N1] = G_DIODE_FEED;
    jacobian[3][HP] = ota_bp;
    jacobian[3][BP] = bp_dn + bp_do - G_BANDPASS_LOAD - G_DIODE_FEED;
    jacobian[3][VD] = G_DIODE_FEED;
    jacobian[4][N1] = G_SUM + c.lead;
    jacobian[4][BP] = ota_lp;
    jacobian[4][LP] = lp_dn + lp_do - G_SUM - c.lead;
    (residual, jacobian)
}

#[derive(Clone, Copy, Default)]
pub struct Wasp;

impl Kernel for Wasp {
    type Response = WaspResponse;
    type Options = ();
    type Coeffs = WaspCoeffs;
    type State = WaspState;
    /// A 5 V input swings a quarter of the rail either way.
    const VOLTS: f32 = 4.0;

    fn coeffs(
        _: WaspResponse,
        cutoff: f32,
        resonance: f32,
        rates: Rates,
        _: Taps,
        _: (),
    ) -> WaspCoeffs {
        let rotation = 1.0 - (1.0 - POT_END) * (resonance.clamp(0.0, 5.0) as f64 / 5.0);
        let wiper = 1.0 / (1.0 / G_SUM + rotation * (1.0 - rotation) * R_POT);
        // The summer's low gain leaves its output short of the sum it is
        // wired for.
        let (gm, go) = SUMMER.conductances();
        let summer_gain =
            G_SUM / (G_SUM + (go + G_SUM + G_OTA_INPUT) * (3.0 * G_SUM + wiper) / (gm - G_SUM));
        // The loop resonates at the OTAs' rate times the root of the gain
        // around it, so the OTAs run faster by as much.
        let g = rates.gain(cutoff) as f64 / (summer_gain * INTEGRATOR_GAINS).sqrt();
        let core = rates.core as f64;
        WaspCoeffs {
            step: g / OTA_SCALE,
            g,
            bias: 2.0 * C_INTEGRATOR * core * g / OTA_SCALE,
            lead: C_LEAD * core,
            wiper,
            wiper_bandpass: wiper * rotation,
            highpass_gain: 1.0 / summer_gain,
            damping: wiper * rotation / G_SUM,
        }
    }

    #[inline]
    fn step(
        response: WaspResponse,
        c: &WaspCoeffs,
        state: &mut WaspState,
        x: f32,
        _: Taps,
    ) -> [f32; 4] {
        let x = x as f64;
        let mut v = state.v;
        for _ in 0..MAX_ITERATIONS {
            let (residual, jacobian) = equations(c, state, x, &v);
            let mut delta = solve(&jacobian, &residual);
            delta[VD] = delta[VD].clamp(-DIODE_STEP, DIODE_STEP);
            let mut largest = 0.0f64;
            for (value, step) in v.iter_mut().zip(delta) {
                *value -= step;
                largest = largest.max(step.abs());
            }
            if largest < TOLERANCE {
                break;
            }
        }
        if !v.iter().all(|value| value.is_finite()) {
            *state = WaspState::default();
            return [0.0; 4];
        }
        let drive = [ota(v[HP]).0, ota(v[BP]).0];
        for (i, out) in [v[BP], v[LP]].into_iter().enumerate() {
            let q = state.q[i] - c.step * (drive[i] + state.drive[i]);
            // An OTA at the end of its swing stops charging its capacitor.
            state.q[i] = (out + q).clamp(NODE_MIN, NODE_MAX) - out;
        }
        state.drive = drive;
        state.lead = v[LP] - v[N1];
        state.v = v;
        // The summer inverts, so each output is taken upside down.
        let (hp, bp, lp) = (-c.highpass_gain * v[HP], -v[BP], -v[LP]);
        let y = match response {
            WaspResponse::Lowpass => lp,
            WaspResponse::Highpass => hp,
            WaspResponse::Bandpass => bp,
            WaspResponse::Notch => hp + lp,
            WaspResponse::Allpass => hp + lp - c.damping * bp,
        };
        let mut out = [0.0; 4];
        out[DB12] = y as f32;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_circuit_rests_at_its_bias_point() {
        assert!(diodes(0.0).0.abs() < 1e-9, "{}", diodes(0.0).0);
        let rates = Rates {
            engine: 48_000.0,
            core: 96_000.0,
        };
        let c = Wasp::coeffs(WaspResponse::Lowpass, 0.0, 2.5, rates, [true; 4], ());
        let mut state = WaspState::default();
        for _ in 0..1000 {
            let y = Wasp::step(WaspResponse::Lowpass, &c, &mut state, 0.0, [true; 4]);
            assert!(y[DB12].abs() < 1e-6, "{}", y[DB12]);
        }
    }
}
