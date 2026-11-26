//! The Wasp filter's circuit: its component values, its node equations and
//! their solve. [`super::wasp`] describes the circuit and drives it.
//!
//! Voltages are in volts above ground, currents in mA, conductances in mS,
//! resistances in kΩ and capacitances in mF.

use super::wasp_parts::{OTA_REFERENCE, Ota, diodes, inverter};

/// R1 from the level pot, fully up.
pub(super) const R_INPUT: f64 = 27.0;
/// The summer's R2, R3 and R4.
pub(super) const G_SUM: f64 = 1.0 / 27.0;
/// An OTA's input network as its inverter's load: 47 kΩ into 1 kΩ and the
/// 27 kΩ bias pair.
pub(super) const G_OTA_INPUT: f64 = 1.0 / 61.5;
/// R13 from the bandpass to the diodes, and R15 on to the pot's capacitor.
pub(super) const G_DIODE_FEED: f64 = 1.0 / 1000.0;
const G_DIODE_RETURN: f64 = 1.0 / 100.0;
/// The resonance pot, and R14 between it and the bandpass.
const R_POT: f64 = 50.0;
const R_POT_SERIES: f64 = 1.0;
/// Least resistance between the pot's wiper and its capacitor.
const R_POT_END: f64 = 1.0e-3;

/// C1 at the input, C2 across the lowpass's feedback, the integrators' C3
/// and C4, and C7 under the resonance pot.
pub(super) const C_INPUT: f64 = 0.22e-3;
pub(super) const C_LEAD: f64 = 100.0e-9;
pub(super) const C_INTEGRATOR: f64 = 330.0e-9;
pub(super) const C_POT: f64 = 0.22e-3;

pub(super) const MAX_ITERATIONS: usize = 12;
/// Volts a Newton step must fall under to end the solve. Each step
/// squares the error, so what is left is far smaller.
const TOLERANCE: f64 = 1.0e-4;
/// Most a Newton step moves a node, and the diode voltage.
const NODE_STEP: f64 = 1.0;
const DIODE_STEP: f64 = 0.1;

/// The unknowns: the summer's input node, its output, each integrator's
/// input node and output, and the voltage across the diodes.
pub(super) const N1: usize = 0;
pub(super) const HP: usize = 1;
pub(super) const N2: usize = 2;
pub(super) const BP: usize = 3;
pub(super) const N3: usize = 4;
pub(super) const LP: usize = 5;
const VD: usize = 6;
pub(super) const UNKNOWNS: usize = 7;

/// Least size of a pivot the solve divides by.
const SINGULAR: f64 = 1.0e-14;

/// One equation in three unknowns: its coefficients and right-hand side.
type Row = ([f64; 3], f64);

/// Eliminates the first unknown of two equations, pivoting on the larger
/// of its coefficients: the pivot equation and what is left of the other.
#[inline]
fn eliminate(a: Row, b: Row) -> Option<(Row, Row)> {
    let (pivot, other) = if a.0[0].abs() >= b.0[0].abs() {
        (a, b)
    } else {
        (b, a)
    };
    if pivot.0[0].abs() < SINGULAR {
        return None;
    }
    let factor = other.0[0] / pivot.0[0];
    Some((
        pivot,
        (
            [
                0.0,
                other.0[1] - factor * pivot.0[1],
                other.0[2] - factor * pivot.0[2],
            ],
            other.1 - factor * pivot.1,
        ),
    ))
}

/// Solves `a · x = b` for the node equations' Jacobian; `None` for a
/// singular system. The diode node's equation gives the diode voltage and
/// the summer's own gives the highpass, each on a pivot that is a sum of
/// conductances out of one node. Each integrator's input node comes from
/// whichever of its two equations weighs it more, since an OTA at a rail
/// can cancel its capacitor's conductance and a railed inverter loses its
/// gain. That leaves the summer's input node, the bandpass and the
/// lowpass.
#[inline]
pub(super) fn solve(
    a: &[[f64; UNKNOWNS]; UNKNOWNS],
    b: &[f64; UNKNOWNS],
) -> Option<[f64; UNKNOWNS]> {
    let diode = 1.0 / a[6][VD];
    let highpass = 1.0 / a[1][HP];
    // An equation's coefficients on `[n1, bp]` and its right-hand side,
    // with the diode voltage and the highpass taken out.
    let reduce = |row: usize| {
        let by_diode = a[row][VD] * diode;
        let by_highpass = a[row][HP] * highpass;
        (
            a[row][N1] - by_diode * a[6][N1] - by_highpass * a[1][N1],
            a[row][BP] - by_diode * a[6][BP],
            b[row] - by_diode * b[6] - by_highpass * b[1],
        )
    };
    let (s_n1, s_bp, s_b) = reduce(0);
    let s_lp = a[0][LP];
    // The bandpass integrator's two equations in `[n2, n1, bp]`.
    let (i_n1, i_bp, i_b) = reduce(2);
    let (o_n1, o_bp, o_b) = reduce(3);
    let (first, (m, m_b)) =
        eliminate(([a[2][N2], i_n1, i_bp], i_b), ([a[3][N2], o_n1, o_bp], o_b))?;
    // The lowpass integrator's in `[n3, n1, bp]`, with the lowpass kept
    // beside each.
    let (pivot, other) = if a[4][N3].abs() >= a[5][N3].abs() {
        (4, 5)
    } else {
        (5, 4)
    };
    if a[pivot][N3].abs() < SINGULAR {
        return None;
    }
    let factor = a[other][N3] / a[pivot][N3];
    let l_n1 = a[other][N1] - factor * a[pivot][N1];
    let l_bp = a[other][BP] - factor * a[pivot][BP];
    let l_lp = a[other][LP] - factor * a[pivot][LP];
    let l_b = b[other] - factor * b[pivot];
    // Three equations in `[n1, bp, lp]`; the middle one has no lowpass.
    let (m_n1, m_bp) = (m[1], m[2]);
    let determinant = s_n1 * m_bp * l_lp - s_bp * m_n1 * l_lp + s_lp * (m_n1 * l_bp - m_bp * l_n1);
    if determinant.abs() < SINGULAR {
        return None;
    }
    let inverse = 1.0 / determinant;
    let n1 = (s_b * m_bp * l_lp - s_bp * m_b * l_lp + s_lp * (m_b * l_bp - m_bp * l_b)) * inverse;
    let bp = (s_n1 * m_b * l_lp - s_b * m_n1 * l_lp + s_lp * (m_n1 * l_b - m_b * l_n1)) * inverse;
    let lp = (s_n1 * (m_bp * l_b - m_b * l_bp) - s_bp * (m_n1 * l_b - m_b * l_n1)
        + s_b * (m_n1 * l_bp - m_bp * l_n1))
        * inverse;
    let mut x = [0.0; UNKNOWNS];
    x[N1] = n1;
    x[BP] = bp;
    x[LP] = lp;
    x[N2] = (first.1 - first.0[1] * n1 - first.0[2] * bp) / first.0[0];
    x[N3] = (b[pivot] - a[pivot][N1] * n1 - a[pivot][BP] * bp - a[pivot][LP] * lp) / a[pivot][N3];
    x[HP] = (b[1] - a[1][N1] * n1) * highpass;
    x[VD] = (b[6] - a[6][N1] * n1 - a[6][BP] * bp) * diode;
    Some(x)
}

/// The resonance network at one pot setting and one companion conductance
/// for its capacitor: the capacitor's node and the wiper as weights on
/// the summer's input node, the bandpass and the diode voltage.
#[derive(Clone, Copy, Default)]
pub(super) struct Network {
    /// From the bandpass to the wiper, through R14 and the pot.
    pub(super) to_bandpass: f64,
    /// The capacitor's own conductance to its history.
    pub(super) self_conductance: f64,
    /// The capacitor's node, then the wiper, each by `[n1, bp, vd]`.
    pub(super) capacitor: [f64; 3],
    pub(super) wiper: [f64; 3],
    /// The wiper's weight on the capacitor's history.
    pub(super) wiper_history: f64,
}

impl Network {
    /// `rotation` runs from 0, the wiper at the bandpass, to 1, the wiper
    /// at the capacitor. `companion` is the capacitor's conductance over
    /// one sample, 0 at DC.
    pub(super) fn new(rotation: f64, companion: f64) -> Self {
        let to_bandpass = 1.0 / (rotation * R_POT + R_POT_SERIES);
        let to_capacitor = 1.0 / ((1.0 - rotation) * R_POT).max(R_POT_END);
        let wiper_sum = to_bandpass + to_capacitor + G_SUM;
        let self_conductance =
            companion + G_DIODE_RETURN + to_capacitor * (to_bandpass + G_SUM) / wiper_sum;
        let capacitor = [
            (G_DIODE_RETURN + to_capacitor * G_SUM / wiper_sum) / self_conductance,
            to_capacitor * to_bandpass / wiper_sum / self_conductance,
            G_DIODE_RETURN / self_conductance,
        ];
        Self {
            to_bandpass,
            self_conductance,
            capacitor,
            wiper: [
                (G_SUM + to_capacitor * capacitor[0]) / wiper_sum,
                (to_bandpass + to_capacitor * capacitor[1]) / wiper_sum,
                to_capacitor * capacitor[2] / wiper_sum,
            ],
            wiper_history: to_capacitor / wiper_sum,
        }
    }
}

/// The circuit's linear parts over one sample: each capacitor as a
/// conductance, all zero at DC.
#[derive(Clone, Copy, Default)]
pub(super) struct Companions {
    /// R1 and C1 in series.
    pub(super) input: f64,
    /// C1's impedance over half a sample.
    pub(super) input_step: f64,
    pub(super) lead: f64,
    pub(super) integrator: f64,
    pub(super) pot: f64,
    pub(super) network: Network,
}

/// What the capacitors carry in from the previous sample.
#[derive(Clone, Copy, Default)]
pub(super) struct History {
    /// C1's voltage, source side positive, and its current.
    pub(super) input: (f64, f64),
    /// C2's voltage, lowpass side positive.
    pub(super) lead: f64,
    /// Each integrator capacitor's voltage, input side positive, and its
    /// current toward the output.
    pub(super) integrator: [(f64, f64); 2],
    /// C7's voltage and the current into it.
    pub(super) pot: (f64, f64),
}

/// What the unknowns leave implicit: the capacitors' currents, and the
/// voltages under the resonance pot.
#[derive(Clone, Copy)]
pub(super) struct Branches {
    /// Current through C1 into the summer.
    pub(super) input: f64,
    /// Current through C2 into the summer.
    pub(super) lead: f64,
    /// Current through each integrator's capacitor toward its output.
    pub(super) integrator: [f64; 2],
    /// C7's voltage, and the wiper's.
    pub(super) pot: f64,
    pub(super) wiper: f64,
}

/// The linear branches at `v`, for the source `x` at the level pot.
#[inline]
pub(super) fn branches(c: &Companions, history: &History, x: f64, v: &[f64; UNKNOWNS]) -> Branches {
    let net = &c.network;
    let pot_history = (c.pot * history.pot.0 + history.pot.1) / net.self_conductance;
    Branches {
        input: c.input * (x - v[N1] - history.input.0 - c.input_step * history.input.1),
        lead: c.lead * (v[LP] - v[N1] - history.lead),
        integrator: [
            c.integrator * (v[N2] - v[BP] - history.integrator[0].0) - history.integrator[0].1,
            c.integrator * (v[N3] - v[LP] - history.integrator[1].0) - history.integrator[1].1,
        ],
        pot: pot_history
            + net.capacitor[0] * v[N1]
            + net.capacitor[1] * v[BP]
            + net.capacitor[2] * v[VD],
        wiper: net.wiper_history * pot_history
            + net.wiper[0] * v[N1]
            + net.wiper[1] * v[BP]
            + net.wiper[2] * v[VD],
    }
}

/// The node equations' residuals and Jacobian at `v`, for the source `x`
/// at the level pot.
#[inline]
pub(super) fn equations(
    c: &Companions,
    otas: &Ota,
    history: &History,
    x: f64,
    v: &[f64; UNKNOWNS],
) -> ([f64; UNKNOWNS], [[f64; UNKNOWNS]; UNKNOWNS]) {
    let (n1, hp, n2, bp, n3, lp, vd) = (v[N1], v[HP], v[N2], v[BP], v[N3], v[LP], v[VD]);
    let net = &c.network;
    let Branches {
        input,
        lead,
        integrator,
        pot,
        wiper,
    } = branches(c, history, x, v);
    let (diode, diode_slope) = diodes(vd);
    // The node between R13, R15 and the diodes.
    let nd = n1 + vd;
    let (i_hp, hp_dn, hp_do) = inverter(n1, hp);
    let (i_bp, bp_dn, bp_do) = inverter(n2, bp);
    let (i_lp, lp_dn, lp_do) = inverter(n3, lp);
    let (ota_bp, ota_bp_source, ota_bp_node) = otas.current(hp, n2);
    let (ota_lp, ota_lp_source, ota_lp_node) = otas.current(bp, n3);
    let residual = [
        input + G_SUM * (hp - n1) + G_SUM * (lp - n1) + lead + G_SUM * (wiper - n1) + diode,
        i_hp - G_SUM * (hp - n1) - G_OTA_INPUT * (hp - OTA_REFERENCE),
        ota_bp - integrator[0],
        i_bp + integrator[0]
            - G_OTA_INPUT * (bp - OTA_REFERENCE)
            - G_DIODE_FEED * (bp - nd)
            - net.to_bandpass * (bp - wiper),
        ota_lp - integrator[1],
        i_lp + integrator[1] - G_SUM * (lp - n1) - lead,
        G_DIODE_FEED * (bp - nd) - G_DIODE_RETURN * (nd - pot) - diode,
    ];
    let g = c.integrator;
    let mut j = [[0.0; UNKNOWNS]; UNKNOWNS];
    j[0][N1] = -c.input - 2.0 * G_SUM - c.lead + G_SUM * (net.wiper[0] - 1.0);
    j[0][HP] = G_SUM;
    j[0][BP] = G_SUM * net.wiper[1];
    j[0][LP] = G_SUM + c.lead;
    j[0][VD] = G_SUM * net.wiper[2] + diode_slope;
    j[1][N1] = hp_dn + G_SUM;
    j[1][HP] = hp_do - G_SUM - G_OTA_INPUT;
    j[2][HP] = ota_bp_source;
    j[2][N2] = ota_bp_node - g;
    j[2][BP] = g;
    j[3][N1] = G_DIODE_FEED + net.to_bandpass * net.wiper[0];
    j[3][N2] = bp_dn + g;
    j[3][BP] = bp_do - g - G_OTA_INPUT - G_DIODE_FEED - net.to_bandpass * (1.0 - net.wiper[1]);
    j[3][VD] = G_DIODE_FEED + net.to_bandpass * net.wiper[2];
    j[4][BP] = ota_lp_source;
    j[4][N3] = ota_lp_node - g;
    j[4][LP] = g;
    j[5][N1] = G_SUM + c.lead;
    j[5][N3] = lp_dn + g;
    j[5][LP] = lp_do - g - G_SUM - c.lead;
    j[6][N1] = -G_DIODE_FEED - G_DIODE_RETURN * (1.0 - net.capacitor[0]);
    j[6][BP] = G_DIODE_FEED + G_DIODE_RETURN * net.capacitor[1];
    j[6][VD] = -G_DIODE_FEED - G_DIODE_RETURN * (1.0 - net.capacitor[2]) - diode_slope;
    (residual, j)
}

/// Newton's method on the node equations from `v`, for at most
/// `iterations` steps. `None` if a step is not finite.
#[inline]
pub(super) fn newton(
    c: &Companions,
    otas: &Ota,
    history: &History,
    x: f64,
    mut v: [f64; UNKNOWNS],
    iterations: usize,
) -> Option<[f64; UNKNOWNS]> {
    for _ in 0..iterations {
        let (residual, jacobian) = equations(c, otas, history, x, &v);
        let delta = solve(&jacobian, &residual)?;
        let mut largest = 0.0f64;
        for (i, (value, step)) in v.iter_mut().zip(delta).enumerate() {
            let limit = if i == VD { DIODE_STEP } else { NODE_STEP };
            let step = step.clamp(-limit, limit);
            *value -= step;
            largest = largest.max(step.abs());
        }
        if !largest.is_finite() {
            return None;
        }
        if largest < TOLERANCE {
            break;
        }
    }
    Some(v)
}
