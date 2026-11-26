//! The EDP Wasp's filter as Doepfer's A-124 builds it: a state-variable
//! loop whose three amplifiers are unbuffered CMOS inverters on a single
//! 12 V supply, a summer and two integrators each fed by an OTA.
//!
//! The circuit, its component values and the models of its parts follow
//! Köper, Holters, Esqueda and Parker, "A Virtual Analog Model of the EDP
//! Wasp VCF" (DAFx 2022); [`super::wasp_parts`] holds the parts. The
//! inverters' low gain lets their input nodes wander, and the integrating
//! capacitors sit between those nodes and the outputs, so an OTA whose
//! output is carried to a rail stops integrating and pulls back. The
//! resonance pot feeds the bandpass to the summer against a capacitor to
//! ground, beside a pair of diodes; a small capacitor across the lowpass's
//! feedback damps the loop more the higher it is tuned. The signal enters
//! and leaves through coupling capacitors, so the filter passes no DC.
//!
//! Ahead of the loop sits one more inverter of the same chip, wired as a
//! unity-gain amplifier. It is what a hot input overdrives: the signal
//! squares up against its rails, and the level pot after it keeps that
//! full swing inside what the loop passes cleanly.
//!
//! Voltages are in volts above ground, currents in mA, conductances in mS,
//! resistances in kΩ and capacitances in mF. Each sample solves the node
//! equations with Newton's method from the previous sample's voltages.

use super::kernel::{DB12, Kernel, Rates, Taps};
use super::wasp_circuit::{
    BP, C_INPUT, C_INTEGRATOR, C_LEAD, C_POT, Companions, G_DIODE_FEED, G_OTA_INPUT, G_SUM, HP,
    History, INPUT_REST, LEVEL, LP, MAX_ITERATIONS, N1, N2, N3, Network, R_INPUT, UNKNOWNS,
    branches, input_amplifier, newton,
};
use super::wasp_parts::{OTA_REFERENCE, Ota};

/// Corner of the 10 µF output capacitors into a 100 kΩ load, in Hz.
const OUTPUT_CORNER: f64 = 0.16;

/// Small-signal transconductance and output conductance of the inverters
/// at rest: the input amplifier, the summer and the bandpass integrator,
/// which share a bias, and the lowpass integrator.
const INPUT_GM: f64 = 5.256;
const INPUT_GO: f64 = 0.354;
const GM: f64 = 5.207;
const GO: f64 = 0.357;
const LOWPASS_GM: f64 = 5.34;
const LOWPASS_GO: f64 = 0.349;
/// The lowpass output at rest with the resonance pot halfway, which the
/// output is taken against.
const LOWPASS_REST: f64 = 5.0;
/// The circuit at rest with the resonance pot halfway, where the solve
/// for any other setting starts.
const REST_GUESS: [f64; UNKNOWNS] = [5.6007, 6.0, 5.6021, 6.0, 5.6753, 4.9921, 0.1987];

/// The input amplifier's gain for a quiet signal, which its low open-loop
/// gain leaves short of the unity it is wired for.
const INPUT_GAIN: f64 = (INPUT_GM - G_SUM) / (INPUT_GM + 2.0 * INPUT_GO + 3.0 * G_SUM);
/// Brings the loop's outputs back to the level of the module's input,
/// past the input amplifier and the level pot.
const OUTPUT_GAIN: f64 = 1.0 / (INPUT_GAIN * LEVEL * 27.0 / R_INPUT);
/// The drive gain at which a 5 V input starts to square up against the
/// input amplifier's rails.
const RAIL_GAIN: f32 = 2.0;

/// A companion conductance that holds the pot's capacitor still.
const SIGNAL_GROUND: f64 = 1.0e12;

const MAX_REST_ITERATIONS: usize = 60;

/// What a Wasp filter module mixes from the loop's three outputs.
#[derive(Clone, Copy)]
pub(crate) enum WaspResponse {
    Lowpass,
    Highpass,
    Bandpass,
    /// The highpass and lowpass summed.
    Notch,
    /// The notch less the bandpass at the weight the loop feeds it back.
    Allpass,
}

#[derive(Clone, Copy, Default)]
pub struct WaspCoeffs {
    companions: Companions,
    otas: Ota,
    /// The resonance pot's rotation.
    rotation: f64,
    /// Brings the highpass to the level of the lowpass, which the summer's
    /// low gain leaves it short of.
    highpass_gain: f64,
    /// The bandpass's weight in the summer beside the lowpass's, for a
    /// quiet signal.
    damping: f64,
    /// The output capacitor's one-pole coefficient.
    output: f64,
}

#[derive(Clone, Copy, Default)]
pub struct WaspState {
    /// Whether the circuit has settled at rest and the rest holds a
    /// solution.
    settled: bool,
    /// The previous sample's solution, and how far it moved from the one
    /// before.
    v: [f64; UNKNOWNS],
    moved: [f64; UNKNOWNS],
    history: History,
    /// The input amplifier's previous output, above its rest.
    input: f64,
    /// The output capacitor's charge.
    output: f64,
}

impl WaspState {
    /// The circuit at rest for `c`'s resonance, every capacitor charged.
    fn at_rest(response: WaspResponse, c: &WaspCoeffs) -> Self {
        let dc = Companions {
            network: Network::new(c.rotation, 0.0),
            ..Companions::default()
        };
        let none = History::default();
        let v =
            newton(&dc, &c.otas, &none, 0.0, REST_GUESS, MAX_REST_ITERATIONS).unwrap_or(REST_GUESS);
        let pot = branches(&dc, &none, 0.0, &v).pot;
        Self {
            settled: true,
            v,
            moved: [0.0; UNKNOWNS],
            history: History {
                input: (-v[N1], 0.0),
                lead: v[LP] - v[N1],
                integrator: [(v[N2] - v[BP], 0.0), (v[N3] - v[LP], 0.0)],
                pot: (pot, 0.0),
            },
            input: 0.0,
            output: c.mix(response, &v),
        }
    }
}

impl WaspCoeffs {
    /// The response's mix of the loop's outputs, each against its rest.
    #[inline]
    fn mix(&self, response: WaspResponse, v: &[f64; UNKNOWNS]) -> f64 {
        // The input amplifier and the summer each invert, so the outputs
        // come out the right way up.
        let hp = self.highpass_gain * (v[HP] - OTA_REFERENCE);
        let bp = v[BP] - OTA_REFERENCE;
        let lp = v[LP] - LOWPASS_REST;
        match response {
            WaspResponse::Lowpass => lp,
            WaspResponse::Highpass => hp,
            WaspResponse::Bandpass => bp,
            WaspResponse::Notch => hp + lp,
            WaspResponse::Allpass => hp + lp - self.damping * bp,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Wasp;

impl Kernel for Wasp {
    type Response = WaspResponse;
    type Options = ();
    type Coeffs = WaspCoeffs;
    type State = WaspState;
    /// A 5 V input swings the input amplifier a little under half the way
    /// to its rails.
    const VOLTS: f32 = 2.0;

    /// The level holds while `drive` carries a 5 V input up to the input
    /// amplifier's rails, and nothing more comes off once it is there.
    fn drive_makeup(gain: f32) -> f32 {
        1.0 / gain.min(RAIL_GAIN)
    }

    fn coeffs(
        _: WaspResponse,
        cutoff: f32,
        resonance: f32,
        rates: Rates,
        _: Taps,
        _: (),
    ) -> WaspCoeffs {
        let rotation = resonance.clamp(0.0, 5.0) as f64 / 5.0;
        let core = rates.core as f64;
        let pot = 2.0 * C_POT * core;
        let network = Network::new(rotation, pot);
        let input_step = 1.0 / (2.0 * C_INPUT * core);
        // For a signal the pot's capacitor is a ground: the wiper as a
        // weight on the summer's input node and on the bandpass.
        let signal = Network::new(rotation, SIGNAL_GROUND);
        // The summer's and the integrators' low gains leave each short of
        // what it is wired for.
        let summer_load = 1.0 / R_INPUT + 2.0 * G_SUM + G_SUM * (1.0 - signal.wiper[0]);
        let summer_gain = G_SUM / (G_SUM + (GO + G_SUM + G_OTA_INPUT) * summer_load / (GM - G_SUM));
        let bandpass_load =
            G_OTA_INPUT + G_DIODE_FEED + signal.to_bandpass * (1.0 - signal.wiper[1]);
        let integrator_gains =
            1.0 / ((1.0 + (GO + bandpass_load) / GM) * (1.0 + (LOWPASS_GO + G_SUM) / LOWPASS_GM));
        // The loop resonates at the OTAs' rate times the root of the gain
        // around it, so the OTAs run faster by as much.
        let g = rates.gain(cutoff) as f64 / (summer_gain * integrator_gains).sqrt();
        let integrator = 2.0 * C_INTEGRATOR * core;
        let radians = 2.0 * core * rates.gain(cutoff) as f64;
        WaspCoeffs {
            companions: Companions {
                input: 1.0 / (R_INPUT + input_step),
                input_step,
                lead: C_LEAD * core,
                integrator,
                pot,
                network,
            },
            otas: Ota::new(Ota::bias_for(g * integrator)),
            rotation,
            highpass_gain: 1.0 / summer_gain,
            // The pot's share, and C2's, which grows with the cutoff.
            damping: signal.wiper[1] + radians * C_LEAD / G_SUM,
            output: 1.0 - (-std::f64::consts::TAU * OUTPUT_CORNER / core).exp(),
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
        if !state.settled {
            *state = WaspState::at_rest(response, c);
        }
        let amplified = input_amplifier(x as f64, state.input + INPUT_REST) - INPUT_REST;
        let k = &c.companions;
        // The solve starts where the last two samples point.
        let start = std::array::from_fn(|i| state.v[i] + state.moved[i]);
        let source = LEVEL * amplified;
        let solved = newton(k, &c.otas, &state.history, source, start, MAX_ITERATIONS)
            .filter(|v| amplified.is_finite() && v.iter().all(|value| value.is_finite()));
        let Some(v) = solved else {
            *state = WaspState::default();
            return [0.0; 4];
        };
        let branches = branches(k, &state.history, source, &v);
        let history = &mut state.history;
        history.input.0 += k.input_step * (branches.input + history.input.1);
        history.input.1 = branches.input;
        history.lead = v[LP] - v[N1];
        history.integrator = [
            (v[N2] - v[BP], branches.integrator[0]),
            (v[N3] - v[LP], branches.integrator[1]),
        ];
        history.pot = (
            branches.pot,
            k.pot * (branches.pot - history.pot.0) - history.pot.1,
        );
        state.moved = std::array::from_fn(|i| v[i] - state.v[i]);
        state.v = v;
        state.input = amplified;
        let mixed = c.mix(response, &v);
        state.output += c.output * (mixed - state.output);
        let mut out = [0.0; 4];
        out[DB12] = ((mixed - state.output) * OUTPUT_GAIN) as f32;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::wasp_circuit::{equations, solve};
    use super::super::wasp_parts::inverter;
    use super::*;

    const RATES: Rates = Rates {
        engine: 48_000.0,
        core: 96_000.0,
    };

    fn coeffs(resonance: f32) -> WaspCoeffs {
        Wasp::coeffs(WaspResponse::Lowpass, 0.0, resonance, RATES, [true; 4], ())
    }

    #[test]
    fn the_circuit_rests_at_its_bias_point() {
        for resonance in [0.0, 1.0, 2.5, 4.0, 5.0] {
            let c = coeffs(resonance);
            let mut state = WaspState::default();
            for _ in 0..2000 {
                let y = Wasp::step(WaspResponse::Lowpass, &c, &mut state, 0.0, [true; 4]);
                assert!(y[DB12].abs() < 1e-6, "resonance {resonance}: {}", y[DB12]);
            }
            // The OTAs hold the summer and the bandpass at their reference.
            assert!((state.v[HP] - OTA_REFERENCE).abs() < 1e-6);
            assert!((state.v[BP] - OTA_REFERENCE).abs() < 1e-6);
        }
    }

    #[test]
    fn the_solve_satisfies_every_equation() {
        let c = coeffs(4.0);
        let state = WaspState::at_rest(WaspResponse::Lowpass, &c);
        // Mid-swing, and with the bandpass integrator's OTA at a rail.
        for v in [
            [5.4, 7.0, 5.9, 4.0, 5.5, 6.5, 0.3],
            [5.7, 2.0, 0.55, 11.5, 5.6, 5.0, -0.2],
        ] {
            let (b, a) = equations(&c.companions, &c.otas, &state.history, 0.3, &v);
            let x = solve(&a, &b).unwrap();
            for row in 0..UNKNOWNS {
                let sum: f64 = (0..UNKNOWNS).map(|k| a[row][k] * x[k]).sum();
                assert!(
                    (sum - b[row]).abs() < 1e-9,
                    "row {row}: {sum} vs {}",
                    b[row]
                );
            }
        }
    }

    #[test]
    fn the_rest_constants_are_the_models_own() {
        let (current, gm, go) = inverter(INPUT_REST, INPUT_REST);
        assert!(current.abs() < 1e-9, "{current}");
        assert!((-gm / INPUT_GM - 1.0).abs() < 0.01 && (-go / INPUT_GO - 1.0).abs() < 0.01);
        let rest = WaspState::at_rest(WaspResponse::Lowpass, &coeffs(2.5)).v;
        for (value, guess) in rest.iter().zip(REST_GUESS) {
            assert!((value - guess).abs() < 1e-3, "{rest:?}");
        }
        assert!((rest[LP] - LOWPASS_REST).abs() < 0.01);
        let (_, gm, go) = inverter(rest[N1], rest[HP]);
        assert!((-gm / GM - 1.0).abs() < 0.01 && (-go / GO - 1.0).abs() < 0.01);
        let (_, gm, go) = inverter(rest[N3], rest[LP]);
        assert!((-gm / LOWPASS_GM - 1.0).abs() < 0.01 && (-go / LOWPASS_GO - 1.0).abs() < 0.01);
    }
}
