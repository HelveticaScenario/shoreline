//! The Wasp filter's solve: Newton's method on the circuit's node
//! equations, with an elimination shaped to their Jacobian.

use super::wasp_circuit::{
    BP, Companions, HP, Jacobian, LP, N1, N2, N3, Sources, UNKNOWNS, VD, equations,
};
use super::wasp_parts::{Ota, SUPPLY};

/// Steps a sample's solve may take.
pub(super) const MAX_ITERATIONS: usize = 12;
/// Volts a Newton step must fall under to end the solve. Each step
/// squares the error, so what is left is far smaller.
const TOLERANCE: f64 = 1.0e-3;
/// The voltages a node can sit at: the rails and a margin past them.
const NODE_RANGE: (f64, f64) = (-3.0, SUPPLY + 3.0);
/// Most a Newton step moves a node, and the diode voltage.
const NODE_STEP: f64 = 1.0;
const DIODE_STEP: f64 = 0.1;
/// Least size of a pivot the solve divides by.
const SINGULAR: f64 = 1.0e-14;

/// Solves `j · x = b`; `None` for a singular system. The diode node's
/// equation gives the diode voltage and the highpass output's gives the
/// highpass, each on a pivot that is a sum of conductances out of one
/// node. Each integrator's input node comes from whichever of its two
/// equations weighs it more, since an OTA at a rail can cancel its
/// capacitor's conductance and a railed inverter loses its gain. That
/// leaves the summer's input node, the bandpass and the lowpass.
#[inline]
fn solve(j: &Jacobian, b: &[f64; UNKNOWNS]) -> Option<[f64; UNKNOWNS]> {
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
    sources: &Sources,
    mut v: [f64; UNKNOWNS],
    iterations: usize,
) -> Option<([f64; UNKNOWNS], bool)> {
    let mut settled = false;
    for _ in 0..iterations {
        let (residual, jacobian) = equations(c, otas, sources, &v);
        let delta = solve(&jacobian, &residual)?;
        let mut largest = 0.0f64;
        for (i, (value, step)) in v.iter_mut().zip(delta).enumerate() {
            let limit = if i == VD { DIODE_STEP } else { NODE_STEP };
            let step = step.clamp(-limit, limit);
            *value -= step;
            largest = largest.max(step.abs());
        }
        if largest < TOLERANCE {
            settled = true;
            break;
        }
    }
    // A step that is not a number leaves one behind it.
    let held = v[VD].is_finite()
        && v[..VD]
            .iter()
            .all(|node| (NODE_RANGE.0..NODE_RANGE.1).contains(node));
    held.then_some((v, settled))
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Jacobian {
        /// The Jacobian times `x`.
        fn times(&self, x: &[f64; UNKNOWNS]) -> [f64; UNKNOWNS] {
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

    #[test]
    fn the_solve_satisfies_every_equation() {
        let c = Companions::at(0.8, 96_000.0);
        let otas = Ota::new(10.0e-6);
        let sources = Sources::new(&c, &Default::default(), 0.3);
        // Mid-swing, and with the bandpass integrator's OTA at a rail.
        for v in [
            [5.4, 7.0, 5.9, 4.0, 5.5, 6.5, 0.3],
            [5.7, 2.0, 0.55, 11.5, 5.6, 5.0, -0.2],
        ] {
            let (b, a) = equations(&c, &otas, &sources, &v);
            let x = solve(&a, &b).unwrap();
            for (row, (sum, b)) in a.times(&x).into_iter().zip(b).enumerate() {
                assert!((sum - b).abs() < 1e-9, "row {row}: {sum} vs {b}");
            }
        }
    }
}
