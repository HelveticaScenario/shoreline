//! The Wasp filter's nonlinear parts: CD4069 inverter, CA3080 OTA and
//! 1N4148 diode pair, each as the model and the parameters fitted to
//! measured chips in Köper, Holters, Esqueda and Parker, "A Virtual Analog
//! Model of the EDP Wasp VCF" (DAFx 2022).
//!
//! Voltages are in volts above ground, currents in mA and conductances in
//! mS. The fits hold between the rails of the 12 V supply they were
//! measured on.

/// The supply every chip runs on.
pub(super) const SUPPLY: f64 = 12.0;
/// The level the OTAs' inputs are biased to, half the supply.
pub(super) const OTA_REFERENCE: f64 = 0.5 * SUPPLY;
const THERMAL_VOLTAGE: f64 = 0.025;

/// A polynomial in rising powers at `x`, and its slope there.
#[inline]
fn polynomial<const N: usize>(coefficients: &[f64; N], x: f64) -> (f64, f64) {
    let (mut value, mut slope) = (0.0, 0.0);
    for &c in coefficients.iter().rev() {
        slope = slope * x + value;
        value = value * x + c;
    }
    (value, slope)
}

/// One MOSFET of an inverter: a square-law device whose gain and threshold
/// are polynomials in its gate voltage.
struct Mosfet<const N: usize> {
    /// Gain in mA/V² by gate-source voltage.
    gain: [f64; N],
    /// Threshold by gate-source voltage.
    threshold: [f64; 3],
    /// Channel-length modulation per volt.
    lambda: f64,
    /// +1 for the N device, −1 for the P device, whose gate-source voltage
    /// is negative.
    polarity: f64,
}

const N_DEVICE: Mosfet<3> = Mosfet {
    gain: [2.2, -1.0471e-1, 3.1252e-3],
    threshold: [0.8655, 0.2212, 0.0149],
    lambda: 1.0e-3,
    polarity: 1.0,
};

const P_DEVICE: Mosfet<4> = Mosfet {
    gain: [-4.9942e-1, -5.1393e-1, -6.7622e-2, -2.7258e-3],
    threshold: [0.6239, -0.1941, 0.0135],
    lambda: 0.06,
    polarity: -1.0,
};

impl<const N: usize> Mosfet<N> {
    /// Channel current for a signed gate-source voltage and the size of
    /// the drain-source voltage, and its slopes in each.
    #[inline]
    fn current(&self, gate: f64, drain: f64) -> (f64, f64, f64) {
        // The polynomials hold their end values past the rails.
        let (low, high) = if self.polarity > 0.0 {
            (0.0, SUPPLY)
        } else {
            (-SUPPLY, 0.0)
        };
        let fitted = gate.clamp(low, high);
        let (gain, mut gain_slope) = polynomial(&self.gain, fitted);
        let (threshold, mut threshold_slope) = polynomial(&self.threshold, fitted);
        if fitted != gate {
            (gain_slope, threshold_slope) = (0.0, 0.0);
        }
        let overdrive = self.polarity * gate - threshold;
        if overdrive <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let overdrive_slope = self.polarity - threshold_slope;
        let modulation = 1.0 + self.lambda * drain;
        if drain <= overdrive {
            let area = (overdrive - 0.5 * drain) * drain;
            (
                gain * area * modulation,
                (gain_slope * area + gain * overdrive_slope * drain) * modulation,
                gain * ((overdrive - drain) * modulation + area * self.lambda),
            )
        } else {
            let area = 0.5 * overdrive * overdrive;
            (
                gain * area * modulation,
                (gain_slope * area + gain * overdrive * overdrive_slope) * modulation,
                gain * area * self.lambda,
            )
        }
    }
}

/// Current an inverter sources into its output at input `vin` and output
/// `vout`, and its slopes in each.
#[inline]
pub(super) fn inverter(vin: f64, vout: f64) -> (f64, f64, f64) {
    let (i_n, n_gate, n_drain) = N_DEVICE.current(vin, vout);
    let (i_p, p_gate, p_drain) = P_DEVICE.current(vin - SUPPLY, SUPPLY - vout);
    (i_p - i_n, p_gate - n_gate, -p_drain - n_drain)
}

/// Volts across an OTA's inputs per volt its input network is driven
/// above [`OTA_REFERENCE`]: 47 kΩ into 1 kΩ and the 27 kΩ bias pair, into
/// the inverting input.
const OTA_INPUT: f64 = -1.0 / 61.5;
/// Scales of the OTA's `tanh` law: its current, and its input voltage.
const OTA_CURRENT_SCALE: f64 = 0.8635;
const OTA_INPUT_SCALE: f64 = 0.9408;
/// `tanh` argument per volt at the input network.
const OTA_DRIVE: f64 = OTA_INPUT * OTA_INPUT_SCALE / (2.0 * THERMAL_VOLTAGE);

/// The current an OTA's output settles at once pulled to a rail, as
/// coefficients of `1`, the bias, the input voltage, the bias squared and
/// bias × input voltage, in amps and volts.
const LOW_RAIL: [f64; 5] = [1.0e-9, 0.679_38, 1.279_87e-7, 101.188_57, 2.598_19];
const HIGH_RAIL: [f64; 5] = [91.7e-9, -1.136_42, -72.0e-9, -170.117_01, -5.4144];
/// Where each rail's current takes over, and how sharply, per volt.
const LOW_RAIL_AT: f64 = 0.6093;
const LOW_RAIL_SHARPNESS: f64 = 30.14;
const HIGH_RAIL_AT: f64 = 10.8105;
const HIGH_RAIL_SHARPNESS: f64 = 16.0255;

/// `tanh(x)` and `1 − tanh²(x)`, exact where `tanh` has reached ±1.
#[inline]
fn tanh_and_slope(x: f64) -> (f64, f64) {
    if x.abs() > 20.0 {
        (x.signum(), 0.0)
    } else {
        let t = x.tanh();
        (t, 1.0 - t * t)
    }
}

/// An OTA at one bias current.
#[derive(Clone, Copy, Default)]
pub(super) struct Ota {
    /// Peak of the `tanh` law, in mA.
    peak: f64,
    /// Each rail's current at no input, in mA, and its slope in the input
    /// network's voltage.
    low: (f64, f64),
    high: (f64, f64),
}

impl Ota {
    /// The bias, in amps, at which a quiet signal at the input network
    /// meets a transconductance of `gm` mS.
    pub(super) fn bias_for(gm: f64) -> f64 {
        gm / (1.0e3 * OTA_CURRENT_SCALE * -OTA_DRIVE)
    }

    /// An OTA biased at `bias` amps.
    pub(super) fn new(bias: f64) -> Self {
        let rail = |p: &[f64; 5]| {
            (
                1.0e3 * (p[0] + p[1] * bias + p[3] * bias * bias),
                1.0e3 * OTA_INPUT * (p[2] + p[4] * bias),
            )
        };
        Self {
            peak: 1.0e3 * OTA_CURRENT_SCALE * bias,
            low: rail(&LOW_RAIL),
            high: rail(&HIGH_RAIL),
        }
    }

    /// Current into the output node at voltage `node`, for the input
    /// network driven at `source`, and its slopes in each. Away from the
    /// rails it is the `tanh` law alone; toward either rail it gives way
    /// to that rail's current, which pulls the node back.
    #[inline]
    pub(super) fn current(&self, source: f64, node: f64) -> (f64, f64, f64) {
        let drive = source - OTA_REFERENCE;
        let (t, t_slope) = tanh_and_slope(OTA_DRIVE * drive);
        let (law, law_slope) = (self.peak * t, self.peak * t_slope * OTA_DRIVE);
        let (high, high_slope) = tanh_and_slope(HIGH_RAIL_SHARPNESS * (node - HIGH_RAIL_AT));
        let (low, low_slope) = tanh_and_slope(LOW_RAIL_SHARPNESS * (node - LOW_RAIL_AT));
        // How far each rail has taken over, from 0 to 1.
        let (at_high, at_low) = (0.5 * (high + 1.0), 0.5 * (1.0 - low));
        let to_high = self.high.0 + self.high.1 * drive - law;
        let to_low = self.low.0 + self.low.1 * drive - law;
        (
            law + to_high * at_high + to_low * at_low,
            law_slope + (self.high.1 - law_slope) * at_high + (self.low.1 - law_slope) * at_low,
            0.5 * (to_high * HIGH_RAIL_SHARPNESS * high_slope
                - to_low * LOW_RAIL_SHARPNESS * low_slope),
        )
    }
}

/// Saturation current in mA, and ideality factor × thermal voltage.
const DIODE_IS: f64 = 2.52e-6;
const DIODE_NVT: f64 = 1.752 * THERMAL_VOLTAGE;
/// Voltage past which the pair's law is held, far beyond what the
/// resistors feeding it can supply.
const DIODE_LIMIT: f64 = 1.2;

/// Current through the antiparallel diode pair at `v` across it, and its
/// slope.
#[inline]
pub(super) fn diodes(v: f64) -> (f64, f64) {
    let e = (v.clamp(-DIODE_LIMIT, DIODE_LIMIT) / DIODE_NVT).exp();
    let inverse = 1.0 / e;
    (
        DIODE_IS * (e - inverse),
        DIODE_IS * (e + inverse) / DIODE_NVT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slope `f` reports at `x` against a finite difference.
    fn check_slope(f: impl Fn(f64) -> (f64, f64), x: f64, what: &str) {
        let h = 1.0e-6;
        let numeric = (f(x + h).0 - f(x - h).0) / (2.0 * h);
        let slope = f(x).1;
        assert!(
            (slope - numeric).abs() < 1.0e-5 * (1.0 + numeric.abs()),
            "{what} at {x}: {slope} vs {numeric}"
        );
    }

    #[test]
    fn slopes_match_the_curves() {
        for step in 0..=48 {
            let v = 0.25 * step as f64 + 0.013;
            for other in [0.3, 2.0, 5.6, 6.0, 9.0, 11.8] {
                check_slope(
                    |x| {
                        let (i, d, _) = inverter(x, other);
                        (i, d)
                    },
                    v,
                    "inverter input",
                );
                check_slope(
                    |x| {
                        let (i, _, d) = inverter(other, x);
                        (i, d)
                    },
                    v,
                    "inverter output",
                );
                let ota = Ota::new(20.0e-6);
                check_slope(
                    |x| {
                        let (i, d, _) = ota.current(x, other);
                        (i, d)
                    },
                    v,
                    "OTA input",
                );
                check_slope(
                    |x| {
                        let (i, _, d) = ota.current(other, x);
                        (i, d)
                    },
                    v,
                    "OTA output",
                );
            }
        }
        for step in -10..=10 {
            check_slope(diodes, 0.07 * step as f64, "diodes");
        }
    }

    /// The unloaded output for an input, by bisection.
    fn transfer(vin: f64) -> f64 {
        let (mut low, mut high) = (0.0, SUPPLY);
        for _ in 0..60 {
            let mid = 0.5 * (low + high);
            if inverter(vin, mid).0 > 0.0 {
                low = mid;
            } else {
                high = mid;
            }
        }
        low
    }

    #[test]
    fn the_inverter_switches_just_below_half_the_supply() {
        // The measured curve: near the rail to 2 V in, a steep fall
        // between 5.5 and 6 V, and a tail that reaches ground by 10 V.
        assert!(transfer(2.0) > 11.5);
        assert!(transfer(5.5) > 7.0);
        assert!(transfer(6.0) < 3.0);
        assert!(transfer(10.0) < 0.1);
    }

    #[test]
    fn an_ota_pulled_to_a_rail_pushes_back() {
        let ota = Ota::new(50.0e-6);
        for source in [4.0, 6.0, 8.0] {
            let (at_ground, ..) = ota.current(source, 0.2);
            let (at_supply, ..) = ota.current(source, 11.6);
            assert!(
                at_ground > 0.02 && at_supply < -0.04,
                "{at_ground} {at_supply}"
            );
        }
        // Mid-rail it follows its tanh law, whatever the output voltage.
        let (mid, ..) = ota.current(5.0, 6.0);
        let (elsewhere, ..) = ota.current(5.0, 3.0);
        assert!(mid > 0.0 && mid == elsewhere);
    }
}
