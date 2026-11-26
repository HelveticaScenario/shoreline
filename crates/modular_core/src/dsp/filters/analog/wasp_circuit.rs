//! The Wasp filter's circuit: its component values and its node equations.
//! [`super::wasp`] describes the circuit and drives it.
//!
//! Voltages are in volts above ground, currents in mA, conductances in mS,
//! resistances in kΩ and capacitances in mF.

use super::wasp_parts::{OTA_REFERENCE, Ota, R_OTA_INPUT, diodes, inverter};

/// R1, in series with the input.
pub(super) const R_INPUT: f64 = 27.0;
/// The summer's R2, R3 and R4.
pub(super) const G_SUM: f64 = 1.0 / 27.0;
/// An OTA's input network as its inverter's load.
pub(super) const G_OTA_INPUT: f64 = 1.0 / R_OTA_INPUT;
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
const C_INPUT: f64 = 0.22e-3;
pub(super) const C_LEAD: f64 = 100.0e-9;
const C_INTEGRATOR: f64 = 330.0e-9;
const C_POT: f64 = 0.22e-3;

/// The unknowns: the summer's input node, its output, each integrator's
/// input node and output, and the voltage across the diodes.
pub(super) const N1: usize = 0;
pub(super) const HP: usize = 1;
pub(super) const N2: usize = 2;
pub(super) const BP: usize = 3;
pub(super) const N3: usize = 4;
pub(super) const LP: usize = 5;
pub(super) const VD: usize = 6;
pub(super) const UNKNOWNS: usize = 7;

/// The resonance network at one pot setting and one companion conductance
/// for its capacitor: the capacitor's node and the wiper as weights on
/// the summer's input node, the bandpass and the diode voltage.
#[derive(Clone, Copy, Default)]
pub(super) struct Network {
    /// From the bandpass to the wiper, through R14 and the pot.
    pub(super) to_bandpass: f64,
    /// The capacitor's node per unit of current its history carries in.
    history: f64,
    /// The capacitor's node, then the wiper, each by `[n1, bp, vd]`.
    capacitor: [f64; 3],
    pub(super) wiper: [f64; 3],
    /// The wiper's weight on the capacitor's history.
    wiper_history: f64,
}

impl Network {
    /// `rotation` runs from 0, the wiper at the bandpass, to 1, the wiper
    /// at the capacitor. `companion` is the capacitor's conductance over
    /// one sample, 0 at DC.
    pub(super) fn new(rotation: f64, companion: f64) -> Self {
        let to_bandpass = 1.0 / (rotation * R_POT + R_POT_SERIES);
        let to_capacitor = 1.0 / ((1.0 - rotation) * R_POT).max(R_POT_END);
        let wiper_sum = to_bandpass + to_capacitor + G_SUM;
        let history =
            1.0 / (companion + G_DIODE_RETURN + to_capacitor * (to_bandpass + G_SUM) / wiper_sum);
        let capacitor = [
            (G_DIODE_RETURN + to_capacitor * G_SUM / wiper_sum) * history,
            to_capacitor * to_bandpass / wiper_sum * history,
            G_DIODE_RETURN * history,
        ];
        Self {
            to_bandpass,
            history,
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

/// The node equations' Jacobian, by equation and unknown. The equations
/// are the currents into the summer's input node, the highpass output,
/// the bandpass integrator's input node, the bandpass output, the lowpass
/// integrator's input node, the lowpass output and the diode node; an
/// entry no field names is zero.
#[derive(Clone, Copy, Default)]
pub(super) struct Jacobian {
    pub(super) summer: Summer,
    /// `[n1, hp]`.
    pub(super) highpass: [f64; 2],
    /// `[hp, n2, bp]`.
    pub(super) bandpass_node: [f64; 3],
    pub(super) bandpass: Bandpass,
    /// `[bp, n3, lp]`.
    pub(super) lowpass_node: [f64; 3],
    /// `[n1, n3, lp]`.
    pub(super) lowpass: [f64; 3],
    /// `[n1, bp, vd]`.
    pub(super) diode: [f64; 3],
}

#[derive(Clone, Copy, Default)]
pub(super) struct Summer {
    pub(super) n1: f64,
    pub(super) hp: f64,
    pub(super) bp: f64,
    pub(super) lp: f64,
    pub(super) vd: f64,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Bandpass {
    pub(super) n1: f64,
    pub(super) n2: f64,
    pub(super) bp: f64,
    pub(super) vd: f64,
}

/// The circuit's linear parts over one sample: each capacitor as a
/// conductance, all zero at DC.
#[derive(Clone, Copy, Default)]
pub(super) struct Companions {
    /// R1 and C1 in series.
    input: f64,
    /// C1's impedance over half a sample.
    input_step: f64,
    /// C2, stepped by backward Euler: it joins two nodes the inverters
    /// drive, where a trapezoidal step rings at the sample rate.
    lead: f64,
    /// C3 or C4.
    pub(super) integrator: f64,
    /// C7.
    pot: f64,
    network: Network,
    /// The Jacobian with every part's slope at zero: the linear parts'
    /// share of it.
    linear: Jacobian,
}

impl Companions {
    /// The circuit over one sample at `rate` Hz, for a pot `rotation`.
    pub(super) fn at(rotation: f64, rate: f64) -> Self {
        let input_step = 1.0 / (2.0 * C_INPUT * rate);
        let pot = 2.0 * C_POT * rate;
        Self::new(
            1.0 / (R_INPUT + input_step),
            input_step,
            C_LEAD * rate,
            2.0 * C_INTEGRATOR * rate,
            pot,
            Network::new(rotation, pot),
        )
    }

    /// The circuit at DC, every capacitor open, for a pot `rotation`.
    pub(super) fn dc(rotation: f64) -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 0.0, Network::new(rotation, 0.0))
    }

    /// The circuit with each capacitor as the conductance given, in the
    /// order of the fields.
    fn new(
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
}

/// What the capacitors carry in from the previous sample.
#[derive(Clone, Copy, Default)]
pub(super) struct History {
    /// C1's voltage, source side positive, and its current.
    input: (f64, f64),
    /// C2's voltage, lowpass side positive.
    lead: f64,
    /// Each integrator capacitor's voltage, input side positive, and its
    /// current toward the output.
    integrator: [(f64, f64); 2],
    /// C7's voltage and the current into it.
    pot: (f64, f64),
}

impl History {
    /// Every capacitor charged to the circuit at rest at `v`, a solution
    /// of `dc`.
    pub(super) fn at_rest(dc: &Companions, v: &[f64; UNKNOWNS]) -> Self {
        let silent = Sources::new(dc, &Self::default(), 0.0);
        Self {
            input: (-v[N1], 0.0),
            lead: v[LP] - v[N1],
            integrator: [(v[N2] - v[BP], 0.0), (v[N3] - v[LP], 0.0)],
            pot: (branches(dc, &silent, v).pot, 0.0),
        }
    }

    /// Carries the sample solved at `v` into the next.
    #[inline]
    pub(super) fn advance(&mut self, c: &Companions, sources: &Sources, v: &[f64; UNKNOWNS]) {
        let branches = branches(c, sources, v);
        self.input.0 += c.input_step * (branches.input + self.input.1);
        self.input.1 = branches.input;
        self.lead = v[LP] - v[N1];
        self.integrator = [
            (v[N2] - v[BP], branches.integrator[0]),
            (v[N3] - v[LP], branches.integrator[1]),
        ];
        self.pot = (
            branches.pot,
            c.pot * (branches.pot - self.pot.0) - self.pot.1,
        );
    }
}

/// What the unknowns leave implicit: the capacitors' currents, and the
/// voltages under the resonance pot.
#[derive(Clone, Copy)]
struct Branches {
    /// Current through C1 into the summer.
    input: f64,
    /// Current through C2 into the summer.
    lead: f64,
    /// Current through each integrator's capacitor toward its output.
    integrator: [f64; 2],
    /// C7's voltage, and the wiper's.
    pot: f64,
    wiper: f64,
}

/// One sample's input and history as each branch sees them with every
/// unknown at zero: what holds through the sample's Newton steps.
#[derive(Clone, Copy)]
pub(super) struct Sources(Branches);

impl Sources {
    /// For the input `x`, in volts.
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
fn branches(c: &Companions, sources: &Sources, v: &[f64; UNKNOWNS]) -> Branches {
    let (net, at_zero) = (&c.network, &sources.0);
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
    sources: &Sources,
    v: &[f64; UNKNOWNS],
) -> ([f64; UNKNOWNS], Jacobian) {
    let (n1, hp, n2, bp, n3, lp, vd) = (v[N1], v[HP], v[N2], v[BP], v[N3], v[LP], v[VD]);
    let Branches {
        input,
        lead,
        integrator,
        pot,
        wiper,
    } = branches(c, sources, v);
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
