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
//! Each sample solves the circuit's node equations with Newton's method,
//! from where the last samples' voltages point.

use super::kernel::{DB12, Kernel, Rates, Taps};
use super::wasp_circuit::{
    BP, C_LEAD, Companions, G_DIODE_FEED, G_OTA_INPUT, G_SUM, HP, History, LP, Network, R_INPUT,
    Sources, UNKNOWNS,
};
use super::wasp_parts::{OTA_REFERENCE, Ota};
use super::wasp_solve::{MAX_ITERATIONS, newton};

/// Corner of the 10 µF output capacitors into a 100 kΩ load, in Hz.
const OUTPUT_CORNER: f64 = 0.16;

/// Small-signal transconductance and output conductance of the inverters
/// at rest: the summer and the bandpass integrator, which share a bias,
/// and the lowpass integrator.
const GM: f64 = 5.207;
const GO: f64 = 0.357;
const LOWPASS_GM: f64 = 5.34;
const LOWPASS_GO: f64 = 0.349;
/// The circuit at rest with the resonance pot halfway, where the solve
/// for any other setting starts.
const REST_GUESS: [f64; UNKNOWNS] = [5.6007, 6.0, 5.6021, 6.0, 5.6753, 4.9921, 0.1987];

/// A companion conductance that holds the pot's capacitor still.
const SIGNAL_GROUND: f64 = 1.0e12;

/// Steps the solve for the circuit at rest may take.
const MAX_REST_ITERATIONS: usize = 60;
/// Most the solve's starting point is carried ahead of the last solution,
/// in volts.
const PREDICTED_STEP: f64 = 0.5;

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
    /// Brings the highpass to the level of the lowpass.
    highpass_match: f64,
    /// The bandpass's weight in the summer beside the lowpass's, for a
    /// quiet signal.
    damping: f64,
    /// The output capacitor's one-pole coefficient.
    output: f64,
}

#[derive(Clone, Copy, Default)]
pub struct WaspState {
    /// Whether the capacitors are charged; unset, the next sample starts
    /// the circuit at rest.
    charged: bool,
    /// The previous sample's solution, how far it moved from the one
    /// before, and how far that move changed from the one before it.
    v: [f64; UNKNOWNS],
    moved: [f64; UNKNOWNS],
    bent: [f64; UNKNOWNS],
    history: History,
    /// The output capacitor's charge.
    output: f64,
}

impl WaspState {
    /// The circuit at rest for `c`'s resonance, every capacitor charged.
    fn at_rest(response: WaspResponse, c: &WaspCoeffs) -> Self {
        let dc = Companions::dc(c.rotation);
        let silent = Sources::new(&dc, &History::default(), 0.0);
        let v = newton(&dc, &c.otas, &silent, REST_GUESS, MAX_REST_ITERATIONS)
            .map_or(REST_GUESS, |(v, _)| v);
        Self {
            charged: true,
            v,
            moved: [0.0; UNKNOWNS],
            bent: [0.0; UNKNOWNS],
            history: History::at_rest(&dc, &v),
            output: c.mix(response, &v),
        }
    }
}

impl WaspCoeffs {
    /// The response's mix of the loop's outputs, each about the OTAs'
    /// reference; the output capacitor takes off what DC that leaves.
    #[inline]
    fn mix(&self, response: WaspResponse, v: &[f64; UNKNOWNS]) -> f64 {
        // The summer inverts, so each output is taken upside down.
        let hp = OTA_REFERENCE - v[HP];
        let bp = OTA_REFERENCE - v[BP];
        let lp = OTA_REFERENCE - v[LP];
        // The summer's low gain leaves the highpass short of the lowpass,
        // and the two cancel at the cutoff only once matched.
        let matched = self.highpass_match * hp;
        match response {
            WaspResponse::Lowpass => lp,
            WaspResponse::Highpass => hp,
            WaspResponse::Bandpass => bp,
            WaspResponse::Notch => matched + lp,
            WaspResponse::Allpass => matched + lp - self.damping * bp,
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
    /// The circuit works in volts.
    const VOLTS: f32 = 1.0;

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
        let companions = Companions::at(rotation, core);
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
        let g = rates.gain(cutoff) as f64;
        let ota_g = g / (summer_gain * integrator_gains).sqrt();
        WaspCoeffs {
            otas: Ota::new(Ota::bias_for(ota_g * companions.integrator)),
            companions,
            rotation,
            highpass_match: 1.0 / summer_gain,
            // The pot's share, and C2's, which grows with the cutoff: its
            // conductance at the cutoff, `2 · rate · g` radians a second.
            damping: signal.wiper[1] + 2.0 * core * g * C_LEAD / G_SUM,
            // A one-pole's coefficient for a corner far below the rate.
            output: std::f64::consts::TAU * OUTPUT_CORNER / core,
        }
    }

    #[inline]
    fn step(
        response: WaspResponse,
        c: &WaspCoeffs,
        state: &mut WaspState,
        x: f32,
        taps: Taps,
    ) -> [f32; 4] {
        if !state.charged {
            *state = WaspState::at_rest(response, c);
        }
        let k = &c.companions;
        let sources = Sources::new(k, &state.history, x as f64);
        let from_last = || newton(k, &c.otas, &sources, state.v, MAX_ITERATIONS);
        // The solve starts where the last three samples point, and from
        // the last one where they point across a jump or lead it astray.
        let ahead: [f64; UNKNOWNS] = std::array::from_fn(|i| state.moved[i] + state.bent[i]);
        let solved = if ahead.iter().all(|step| step.abs() < PREDICTED_STEP) {
            let start = std::array::from_fn(|i| state.v[i] + ahead[i]);
            newton(k, &c.otas, &sources, start, MAX_ITERATIONS)
                .filter(|&(_, settled)| settled)
                .or_else(from_last)
        } else {
            from_last()
        };
        let Some((v, _)) = solved else {
            *state = WaspState::default();
            return [0.0; 4];
        };
        state.history.advance(k, &sources, &v);
        let moved: [f64; UNKNOWNS] = std::array::from_fn(|i| v[i] - state.v[i]);
        state.bent = std::array::from_fn(|i| moved[i] - state.moved[i]);
        state.moved = moved;
        state.v = v;
        let mixed = c.mix(response, &v);
        state.output += c.output * (mixed - state.output);
        let mut out = [0.0; 4];
        if taps[DB12] {
            out[DB12] = (mixed - state.output) as f32;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::wasp_circuit::{N1, N3};
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
    fn the_rest_constants_are_the_models_own() {
        let rest = WaspState::at_rest(WaspResponse::Lowpass, &coeffs(2.5)).v;
        for (value, guess) in rest.iter().zip(REST_GUESS) {
            assert!((value - guess).abs() < 1e-3, "{rest:?}");
        }
        let (_, gm, go) = inverter(rest[N1], rest[HP]);
        assert!((-gm / GM - 1.0).abs() < 0.01 && (-go / GO - 1.0).abs() < 0.01);
        let (_, gm, go) = inverter(rest[N3], rest[LP]);
        assert!((-gm / LOWPASS_GM - 1.0).abs() < 0.01 && (-go / LOWPASS_GO - 1.0).abs() < 0.01);
    }
}
