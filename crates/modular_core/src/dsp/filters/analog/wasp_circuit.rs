//! The Wasp filter's circuit: its component values, its node equations and
//! their solve. [`super::wasp`] describes the circuit and drives it.
//!
//! Voltages are in volts above ground, currents in mA, conductances in mS,
//! resistances in kΩ and capacitances in mF.

use super::wasp_network::Network;
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
pub(super) const G_DIODE_RETURN: f64 = 1.0 / 100.0;

/// C1 at the input, C2 across the lowpass's feedback, the integrators' C3
/// and C4, and C7 under the resonance pot.
pub(super) const C_INPUT: f64 = 0.22e-3;
pub(super) const C_LEAD: f64 = 100.0e-9;
pub(super) const C_INTEGRATOR: f64 = 330.0e-9;
pub(super) const C_POT: f64 = 0.22e-3;

pub(super) const MAX_ITERATIONS: usize = 12;
/// Volts a Newton step must fall under to end the solve. Each step
/// squares the error, so what is left is far smaller.
const TOLERANCE: f64 = 1.0e-3;
/// The voltages a node can sit at: the rails and a margin past them.
const NODE_RANGE: (f64, f64) = (-3.0, 15.0);
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

/// The node equations' Jacobian, by equation and unknown. The equations
/// are the currents into the summer's input node, the highpass output,
/// the bandpass integrator's input node, the bandpass output, the lowpass
/// integrator's input node, the lowpass output and the diode node; an
/// entry no field names is zero.
#[derive(Clone, Copy, Default)]
pub(super) struct Jacobian {
    summer: Summer,
    /// `[n1, hp]`.
    highpass: [f64; 2],
    /// `[hp, n2, bp]`.
    bandpass_node: [f64; 3],
    bandpass: Bandpass,
    /// `[bp, n3, lp]`.
    lowpass_node: [f64; 3],
    /// `[n1, n3, lp]`.
    lowpass: [f64; 3],
    /// `[n1, bp, vd]`.
    diode: [f64; 3],
}

#[derive(Clone, Copy, Default)]
struct Summer {
    n1: f64,
    hp: f64,
    bp: f64,
    lp: f64,
    vd: f64,
}

#[derive(Clone, Copy, Default)]
struct Bandpass {
    n1: f64,
    n2: f64,
    bp: f64,
    vd: f64,
}

impl Jacobian {
    /// The Jacobian times `x`.
    #[cfg(test)]
    pub(super) fn times(&self, x: &[f64; UNKNOWNS]) -> [f64; UNKNOWNS] {
        let (s, b) = (&self.summer, &self.bandpass);
        [
            s.n1 * x[N1] + s.hp * x[HP] + s.bp * x[BP] + s.lp * x[LP] + s.vd * x[VD],
            self.highpass[0] * x[N1] + self.highpass[1] * x[HP],
            self.bandpass_node[0] * x[HP]
                + self.bandpass_node[1] * x[N2]
                + self.bandpass_node[2] * x[BP],
            b.n1 * x[N1] + b.n2 * x[N2] + b.bp * x[BP] + b.vd * x[VD],
            self.lowpass_node[0] * x[BP]
                + self.lowpass_node[1] * x[N3]
                + self.lowpass_node[2] * x[LP],
            self.lowpass[0] * x[N1] + self.lowpass[1] * x[N3] + self.lowpass[2] * x[LP],
            self.diode[0] * x[N1] + self.diode[1] * x[BP] + self.diode[2] * x[VD],
        ]
    }
}

/// The circuit's linear parts over one sample: each capacitor as a
/// conductance, all zero at DC.
#[derive(Clone, Copy, Default)]
pub(super) struct Companions {
    /// R1 and C1 in series.
    input: f64,
    /// C1's impedance over half a sample.
    pub(super) input_step: f64,
    lead: f64,
    integrator: f64,
    pub(super) pot: f64,
    network: Network,
    /// The Jacobian with every part's slope at zero: the linear parts'
    /// share of it.
    linear: Jacobian,
}

impl Companions {
    /// The circuit with its capacitors as the conductances given: R1 and
    /// C1 in series, C1's impedance over half a sample, then C2, an
    /// integrator's capacitor and C7.
    pub(super) fn new(
        input: f64,
        input_step: f64,
        lead: f64,
        integrator: f64,
        pot: f64,
        network: Network,
    ) -> Self {
        let net = &network;
        let g = integrator;
        Self {
            input,
            input_step,
            lead,
            integrator,
            pot,
            network,
            linear: Jacobian {
                summer: Summer {
                    n1: -input - 2.0 * G_SUM - lead + G_SUM * (net.wiper[0] - 1.0),
                    hp: G_SUM,
                    bp: G_SUM * net.wiper[1],
                    lp: G_SUM + lead,
                    vd: G_SUM * net.wiper[2],
                },
                highpass: [G_SUM, -G_SUM - G_OTA_INPUT],
                bandpass_node: [0.0, -g, g],
                bandpass: Bandpass {
                    n1: G_DIODE_FEED + net.to_bandpass * net.wiper[0],
                    n2: g,
                    bp: -g - G_OTA_INPUT - G_DIODE_FEED - net.to_bandpass * (1.0 - net.wiper[1]),
                    vd: G_DIODE_FEED + net.to_bandpass * net.wiper[2],
                },
                lowpass_node: [0.0, -g, g],
                lowpass: [G_SUM + lead, g, -g - G_SUM - lead],
                diode: [
                    -G_DIODE_FEED - G_DIODE_RETURN * (1.0 - net.capacitor[0]),
                    G_DIODE_FEED + G_DIODE_RETURN * net.capacitor[1],
                    -G_DIODE_FEED - G_DIODE_RETURN * (1.0 - net.capacitor[2]),
                ],
            },
        }
    }

    /// The circuit at DC, every capacitor open, for a pot `rotation`.
    pub(super) fn dc(rotation: f64) -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 0.0, Network::new(rotation, 0.0))
    }
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

/// One sample's source and history as each branch sees them with every
/// unknown at zero: what stays put through the sample's Newton steps.
#[derive(Clone, Copy)]
pub(super) struct Sample(Branches);

impl Sample {
    /// For the source `x` at the level pot.
    #[inline]
    pub(super) fn new(c: &Companions, history: &History, x: f64) -> Self {
        let pot = (c.pot * history.pot.0 + history.pot.1) * c.network.history;
        Self(Branches {
            input: c.input * (x - history.input.0 - c.input_step * history.input.1),
            lead: -c.lead * history.lead,
            integrator: [
                -c.integrator * history.integrator[0].0 - history.integrator[0].1,
                -c.integrator * history.integrator[1].0 - history.integrator[1].1,
            ],
            pot,
            wiper: c.network.wiper_history * pot,
        })
    }
}

/// The linear branches at `v`.
#[inline]
pub(super) fn branches(c: &Companions, sample: &Sample, v: &[f64; UNKNOWNS]) -> Branches {
    let (net, at_zero) = (&c.network, &sample.0);
    Branches {
        input: at_zero.input - c.input * v[N1],
        lead: at_zero.lead + c.lead * (v[LP] - v[N1]),
        integrator: [
            at_zero.integrator[0] + c.integrator * (v[N2] - v[BP]),
            at_zero.integrator[1] + c.integrator * (v[N3] - v[LP]),
        ],
        pot: at_zero.pot
            + net.capacitor[0] * v[N1]
            + net.capacitor[1] * v[BP]
            + net.capacitor[2] * v[VD],
        wiper: at_zero.wiper + net.wiper[0] * v[N1] + net.wiper[1] * v[BP] + net.wiper[2] * v[VD],
    }
}

/// The node equations' residuals and Jacobian at `v`.
#[inline]
pub(super) fn equations(
    c: &Companions,
    otas: &Ota,
    sample: &Sample,
    v: &[f64; UNKNOWNS],
) -> ([f64; UNKNOWNS], Jacobian) {
    let (n1, hp, n2, bp, n3, lp, vd) = (v[N1], v[HP], v[N2], v[BP], v[N3], v[LP], v[VD]);
    let Branches {
        input,
        lead,
        integrator,
        pot,
        wiper,
    } = branches(c, sample, v);
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
            - c.network.to_bandpass * (bp - wiper),
        ota_lp - integrator[1],
        i_lp + integrator[1] - G_SUM * (lp - n1) - lead,
        G_DIODE_FEED * (bp - nd) - G_DIODE_RETURN * (nd - pot) - diode,
    ];
    let mut j = c.linear;
    j.summer.vd += diode_slope;
    j.highpass[0] += hp_dn;
    j.highpass[1] += hp_do;
    j.bandpass_node[0] = ota_bp_source;
    j.bandpass_node[1] += ota_bp_node;
    j.bandpass.n2 += bp_dn;
    j.bandpass.bp += bp_do;
    j.lowpass_node[0] = ota_lp_source;
    j.lowpass_node[1] += ota_lp_node;
    j.lowpass[1] += lp_dn;
    j.lowpass[2] += lp_do;
    j.diode[2] -= diode_slope;
    (residual, j)
}

/// Solves `j · x = b`; `None` for a singular system. The diode node's
/// equation gives the diode voltage and the highpass output's gives the
/// highpass, each on a pivot that is a sum of conductances out of one
/// node. Each integrator's input node comes from whichever of its two
/// equations weighs it more, since an OTA at a rail can cancel its
/// capacitor's conductance and a railed inverter loses its gain. That
/// leaves the summer's input node, the bandpass and the lowpass.
#[inline]
pub(super) fn solve(j: &Jacobian, b: &[f64; UNKNOWNS]) -> Option<[f64; UNKNOWNS]> {
    let diode = 1.0 / j.diode[2];
    let highpass = 1.0 / j.highpass[1];
    // The summer's equation in `[n1, bp, lp]`.
    let by_diode = j.summer.vd * diode;
    let by_highpass = j.summer.hp * highpass;
    let s_n1 = j.summer.n1 - by_diode * j.diode[0] - by_highpass * j.highpass[0];
    let s_bp = j.summer.bp - by_diode * j.diode[1];
    let s_lp = j.summer.lp;
    let s_b = b[0] - by_diode * b[6] - by_highpass * b[1];
    // The bandpass integrator's two equations in `[n2, n1, bp]`.
    let by_highpass = j.bandpass_node[0] * highpass;
    let node = (
        j.bandpass_node[1],
        -by_highpass * j.highpass[0],
        j.bandpass_node[2],
        b[2] - by_highpass * b[1],
    );
    let by_diode = j.bandpass.vd * diode;
    let output = (
        j.bandpass.n2,
        j.bandpass.n1 - by_diode * j.diode[0],
        j.bandpass.bp - by_diode * j.diode[1],
        b[3] - by_diode * b[6],
    );
    let (first, other) = if node.0.abs() >= output.0.abs() {
        (node, output)
    } else {
        (output, node)
    };
    if first.0.abs() < SINGULAR {
        return None;
    }
    let first_pivot = 1.0 / first.0;
    let factor = other.0 * first_pivot;
    let m_n1 = other.1 - factor * first.1;
    let m_bp = other.2 - factor * first.2;
    let m_b = other.3 - factor * first.3;
    // The lowpass integrator's in `[n3, n1, bp, lp]`.
    let node = (
        j.lowpass_node[1],
        0.0,
        j.lowpass_node[0],
        j.lowpass_node[2],
        b[4],
    );
    let output = (j.lowpass[1], j.lowpass[0], 0.0, j.lowpass[2], b[5]);
    let (second, other) = if node.0.abs() >= output.0.abs() {
        (node, output)
    } else {
        (output, node)
    };
    if second.0.abs() < SINGULAR {
        return None;
    }
    let second_pivot = 1.0 / second.0;
    let factor = other.0 * second_pivot;
    let l_n1 = other.1 - factor * second.1;
    let l_bp = other.2 - factor * second.2;
    let l_lp = other.3 - factor * second.3;
    let l_b = other.4 - factor * second.4;
    // Three equations in `[n1, bp, lp]`; the middle one has no lowpass.
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
    x[N2] = (first.3 - first.1 * n1 - first.2 * bp) * first_pivot;
    x[N3] = (second.4 - second.1 * n1 - second.2 * bp - second.3 * lp) * second_pivot;
    x[HP] = (b[1] - j.highpass[0] * n1) * highpass;
    x[VD] = (b[6] - j.diode[0] * n1 - j.diode[1] * bp) * diode;
    Some(x)
}

/// Newton's method on the node equations from `v`, for at most
/// `iterations` steps: where it ends, and whether it settled there. `None`
/// if it leaves the voltages a circuit on this supply can hold.
#[inline]
pub(super) fn newton(
    c: &Companions,
    otas: &Ota,
    sample: &Sample,
    mut v: [f64; UNKNOWNS],
    iterations: usize,
) -> Option<([f64; UNKNOWNS], bool)> {
    let mut settled = false;
    for _ in 0..iterations {
        let (residual, jacobian) = equations(c, otas, sample, &v);
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
            settled = true;
            break;
        }
    }
    v[..VD]
        .iter()
        .all(|node| (NODE_RANGE.0..NODE_RANGE.1).contains(node))
        .then_some((v, settled))
}
