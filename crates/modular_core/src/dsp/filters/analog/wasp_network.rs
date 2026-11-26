//! The Wasp filter's resonance network: the pot between the bandpass and
//! its capacitor to ground, with the wiper feeding the summer.

use super::wasp_circuit::{G_DIODE_RETURN, G_SUM};

/// The resonance pot, and R14 between it and the bandpass.
const R_POT: f64 = 50.0;
const R_POT_SERIES: f64 = 1.0;
/// Least resistance between the pot's wiper and its capacitor.
const R_POT_END: f64 = 1.0e-3;

/// The resonance network at one pot setting and one companion conductance
/// for its capacitor: the capacitor's node and the wiper as weights on
/// the summer's input node, the bandpass and the diode voltage.
#[derive(Clone, Copy, Default)]
pub(super) struct Network {
    /// From the bandpass to the wiper, through R14 and the pot.
    pub(super) to_bandpass: f64,
    /// The capacitor's node per unit of current its history carries in.
    pub(super) history: f64,
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
