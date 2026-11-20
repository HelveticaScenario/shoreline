//! Strudel's wavetable warp modes.
//!
//! Ported from Strudel's wavetable oscillator (`packages/superdough/worklets.mjs`,
//! <https://codeberg.org/uzu/strudel>), by the Strudel contributors. Used under the
//! GNU AGPL v3.
//!
//! Each mode takes Strudel's warp amount `amt` in `[0, 1]`, a 0–5 knob over five.
//! [`prepare`] turns it into the mode's constants once per lane and sample, and
//! [`warp`] reshapes each phase with them. Like Strudel's, none of these is
//! antialiased.
//!
//! Allocation-free — safe to call from the audio thread.

use std::f32::consts::TAU;

use super::shapes::knob;

/// The Strudel modes. `Flip` keeps the phase and inverts the wave read with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Asym,
    Mirror,
    Bendp,
    Bendm,
    Bendmp,
    Sync,
    Quant,
    Fold,
    Pwm,
    Orbit,
    Spin,
    Chaos,
    Primes,
    Binary,
    Brownian,
    Reciprocal,
    Wormhole,
    Logistic,
    Sigmoid,
    Fractal,
    Flip,
}

/// A mode's constants for one amount; what each field holds depends on the mode.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Prepared {
    pub a: f32,
    pub b: f32,
    pub c: f32,
}

impl Prepared {
    pub(super) fn one(a: f32) -> Self {
        Self { a, b: 0.0, c: 0.0 }
    }
}

/// JavaScript's `x | 0`: truncation toward zero.
#[inline]
fn ffloor(x: f32) -> f32 {
    x.trunc()
}

/// Strudel's `fround`: `(x + 0.5) | 0`.
#[inline]
fn fround(x: f32) -> f32 {
    (x + 0.5).trunc()
}

#[inline]
fn frac(x: f32) -> f32 {
    x - x.floor()
}

/// Strudel's `_toBits`: a bit depth falling from `max` to `min` as `amt`
/// rises, and the step count `2^bits`, rounded.
#[inline]
fn to_bits(amt: f32, min: f32, max: f32) -> (f32, f32) {
    let bits = max + (min - max) * amt;
    (bits, fround(bits.exp2()))
}

fn is_prime(n: u32) -> bool {
    if n < 2 {
        return false;
    }
    if n.is_multiple_of(2) {
        return n == 2;
    }
    let mut d = 3;
    while d * d <= n {
        if n.is_multiple_of(d) {
            return false;
        }
        d += 2;
    }
    true
}

/// The knee of `asym` and `pwm`: `x` below `knee` maps to the first half.
#[inline]
fn knee(x: f32, knee: f32) -> f32 {
    if x < knee || knee >= 1.0 {
        0.5 * x / knee
    } else {
        0.5 + 0.5 * (x - knee) / (1.0 - knee)
    }
}

/// Strudel's integer hash, with JavaScript's 32-bit wrapping arithmetic.
#[inline]
fn hash32(u: u32) -> u32 {
    let mut u = u.wrapping_add(0x7ed5_5d16).wrapping_add(u << 12);
    u = u ^ 0xc761_c23c ^ (u >> 19);
    u = u.wrapping_add(0x1656_67b1).wrapping_add(u << 5);
    u = u.wrapping_add(0xd3a2_646c) ^ (u << 9);
    u = u.wrapping_add(0xfd70_46c5).wrapping_add(u << 3);
    u ^ 0xb55a_4f09 ^ (u >> 16)
}

#[inline]
fn hash01(i: u32) -> f32 {
    (hash32(i) >> 8) as f32 / 16_777_216.0
}

/// Value noise: hashed values at the integers, linearly interpolated.
#[inline]
fn noise(x: f32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let i = i as u32;
    let (a, b) = (hash01(i), hash01(i.wrapping_add(1)));
    a + (b - a) * f
}

/// Four octaves of value noise, in `[-1, 1]`. `x` is non-negative.
#[inline]
fn brownian(x: f32) -> f32 {
    let (mut amp, mut sum, mut norm, mut freq) = (0.5, 0.0, 0.0, 1.0);
    for _ in 0..4 {
        sum += amp * noise(x * freq);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    (sum / norm) * 2.0 - 1.0
}

#[inline]
fn bit_reverse(mut i: u32, bits: u32) -> u32 {
    let mut r = 0;
    for _ in 0..bits {
        r = (r << 1) | (i & 1);
        i >>= 1;
    }
    r
}

/// A mode's constants for a 0–5 knob value.
pub fn prepare(mode: Mode, knob_value: f32) -> Prepared {
    let amt = knob(knob_value);
    match mode {
        Mode::Asym | Mode::Mirror => Prepared::one(0.01 + 0.99 * amt),
        Mode::Bendp => Prepared::one(1.0 + 3.0 * amt),
        Mode::Bendm => Prepared::one(1.0 / (1.0 + 3.0 * amt)),
        // Bend down below the middle of the knob, up above it.
        Mode::Bendmp => Prepared::one(if amt < 0.5 {
            1.0 / (1.0 + 3.0 * (1.0 - 2.0 * amt))
        } else {
            1.0 + 3.0 * (2.0 * amt - 1.0)
        }),
        Mode::Sync => Prepared::one(16f32.powf(amt * amt)),
        Mode::Quant => Prepared::one(to_bits(amt, 2.0, 12.0).1),
        Mode::Fold => Prepared::one(1.0 + fround(7.0 * amt).max(1.0)),
        Mode::Pwm => Prepared::one((0.5 + 0.49 * (2.0 * amt - 1.0)).clamp(0.0, 1.0)),
        Mode::Orbit | Mode::Fractal => Prepared::one(0.5 * amt),
        Mode::Spin => Prepared {
            a: 0.5 * amt,
            b: to_bits(amt, 1.0, 6.0).1,
            c: 0.0,
        },
        Mode::Chaos => Prepared {
            a: amt,
            b: 3.7 + 0.3 * amt,
            c: 0.0,
        },
        Mode::Primes => {
            let mut n = to_bits(amt, 3.0, 12.0).1 as u32;
            while !is_prime(n) {
                n += 1;
            }
            Prepared::one(n as f32)
        }
        Mode::Binary => {
            let bits = fround(to_bits(amt, 3.0, 12.0).0);
            Prepared {
                a: (1u32 << bits as u32) as f32,
                b: bits,
                c: 0.0,
            }
        }
        Mode::Brownian => Prepared::one(0.25 * amt),
        Mode::Reciprocal => Prepared::one(2.0 + 4.0 * amt),
        Mode::Wormhole => {
            let gap = (0.8 * amt).clamp(0.0, 1.0);
            Prepared {
                a: 0.5 * (1.0 - gap),
                b: 0.5 * (1.0 + gap),
                c: 0.0,
            }
        }
        Mode::Logistic => Prepared {
            a: 3.6 + 0.4 * amt,
            b: 1.0 + fround(2.0 * amt),
            c: 0.0,
        },
        Mode::Sigmoid => {
            let k = 1.0 + 10.0 * amt;
            Prepared {
                a: k,
                b: 1.0 / (1.0 + (0.5 * k).exp()),
                c: 1.0 / (1.0 + (-0.5 * k).exp()),
            }
        }
        Mode::Flip => Prepared::one(amt),
    }
}

/// Warp phase `x` in `[0, 1]` with `mode`'s constants `p`. Returns the new
/// phase and the sign to read the wave with: -1 where `flip` inverts it.
#[inline]
pub fn warp(mode: Mode, x: f32, p: &Prepared) -> (f32, f32) {
    let y = match mode {
        Mode::Asym => knee(x, p.a),
        Mode::Mirror => 1.0 - (2.0 * knee(x, p.a) - 1.0).abs(),
        Mode::Bendp | Mode::Bendm | Mode::Bendmp => x.powf(p.a),
        Mode::Sync => frac(x * p.a),
        Mode::Quant | Mode::Primes => ffloor(x * p.a) / p.a,
        Mode::Fold => ((x * p.a - ffloor(x * p.a)) - 0.5).abs() * 2.0,
        Mode::Pwm => knee(x, p.a),
        Mode::Orbit => frac(x + p.a * (TAU * 3.0 * x).sin()),
        Mode::Spin => frac(x + p.a * (TAU * p.b * x).sin()),
        Mode::Chaos => ((1.0 - p.a) * x + p.a * p.b * x * (1.0 - x)).clamp(0.0, 1.0),
        Mode::Binary => {
            let idx = ffloor(x * p.a) as u32;
            bit_reverse(idx, p.b as u32) as f32 / p.a
        }
        Mode::Brownian => frac(x + p.a * brownian(64.0 * x)),
        Mode::Reciprocal => {
            let den = x + (1.0 - x) * p.a;
            let y = if den > 1e-12 { x * p.a / den } else { 0.0 };
            y.clamp(0.0, 1.0)
        }
        Mode::Wormhole => {
            if x < p.a {
                0.5 * x / p.a
            } else if x > p.b {
                0.5 * (1.0 + (x - p.b) / (1.0 - p.b))
            } else {
                0.5
            }
        }
        Mode::Logistic => {
            let mut y = x;
            for _ in 0..p.b as u32 {
                y = p.a * y * (1.0 - y);
            }
            y.clamp(0.0, 1.0)
        }
        Mode::Sigmoid => {
            let y = 1.0 / (1.0 + (-p.a * (x - 0.5)).exp());
            (y - p.b) / (p.c - p.b)
        }
        Mode::Fractal => frac(x + p.a * (TAU * x).sin()),
        Mode::Flip => return (x, if x < p.a { -1.0 } else { 1.0 }),
    };
    (y, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Mode; 21] = [
        Mode::Asym,
        Mode::Mirror,
        Mode::Bendp,
        Mode::Bendm,
        Mode::Bendmp,
        Mode::Sync,
        Mode::Quant,
        Mode::Fold,
        Mode::Pwm,
        Mode::Orbit,
        Mode::Spin,
        Mode::Chaos,
        Mode::Primes,
        Mode::Binary,
        Mode::Brownian,
        Mode::Reciprocal,
        Mode::Wormhole,
        Mode::Logistic,
        Mode::Sigmoid,
        Mode::Fractal,
        Mode::Flip,
    ];

    fn at(mode: Mode, knob_value: f32, x: f32) -> f32 {
        warp(mode, x, &prepare(mode, knob_value)).0
    }

    #[test]
    fn every_mode_stays_in_range() {
        for mode in ALL {
            for knob_value in [0.0, 1.0, 2.5, 4.0, 5.0] {
                for i in 0..=200 {
                    let x = i as f32 / 200.0;
                    let y = at(mode, knob_value, x);
                    assert!(
                        (0.0..=1.0).contains(&y),
                        "{mode:?} at knob {knob_value}, x {x}: {y}"
                    );
                }
            }
        }
    }

    #[test]
    fn hash_matches_javascript() {
        // Strudel's `hash32` for these inputs, evaluated in Node.
        assert_eq!(hash32(0), 0x6b4e_d927);
        assert_eq!(hash32(1), 0xb486_81b6);
        assert_eq!(hash32(12345), 0xb652_3c53);
        assert_eq!(hash32(4_000_000), 0x83dc_29d7);
    }

    #[test]
    fn sync_runs_up_to_sixteen_times_faster() {
        assert_eq!(prepare(Mode::Sync, 0.0).a, 1.0);
        assert_eq!(prepare(Mode::Sync, 5.0).a, 16.0);
        assert!((at(Mode::Sync, 5.0, 0.5 / 16.0) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn fold_folds_two_to_eight_times() {
        assert_eq!(prepare(Mode::Fold, 0.0).a, 2.0);
        assert_eq!(prepare(Mode::Fold, 5.0).a, 8.0);
        // A whole number of folds lands back where it started.
        assert_eq!(at(Mode::Fold, 0.0, 0.0), 1.0);
        assert_eq!(at(Mode::Fold, 0.0, 0.25), 0.0);
    }

    #[test]
    fn pwm_is_even_at_the_middle_of_the_knob() {
        for i in 0..=10 {
            let x = i as f32 / 10.0;
            assert!((at(Mode::Pwm, 2.5, x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn mirror_peaks_at_the_knee() {
        let knee = prepare(Mode::Mirror, 2.5).a;
        assert!((at(Mode::Mirror, 2.5, knee) - 1.0).abs() < 1e-5);
        assert_eq!(at(Mode::Mirror, 2.5, 0.0), 0.0);
    }

    #[test]
    fn bendmp_is_linear_at_the_middle_and_bends_either_way() {
        assert!((at(Mode::Bendmp, 2.5, 0.3) - 0.3).abs() < 1e-6);
        assert!(at(Mode::Bendmp, 0.0, 0.3) > 0.3);
        assert!(at(Mode::Bendmp, 5.0, 0.3) < 0.3);
        assert_eq!(at(Mode::Bendmp, 0.0, 0.3), at(Mode::Bendm, 5.0, 0.3));
        assert_eq!(at(Mode::Bendmp, 5.0, 0.3), at(Mode::Bendp, 5.0, 0.3));
    }

    #[test]
    fn primes_steps_a_prime_number_of_times() {
        for knob_value in [0.0, 1.3, 2.5, 3.9, 5.0] {
            let n = prepare(Mode::Primes, knob_value).a as u32;
            assert!(is_prime(n), "{n} at knob {knob_value}");
        }
        assert_eq!(prepare(Mode::Primes, 5.0).a, 11.0);
    }

    #[test]
    fn binary_reverses_the_step_index() {
        // 3 bits at full amount: step 1 of 8 (001) reads step 4 (100).
        assert_eq!(prepare(Mode::Binary, 5.0).b, 3.0);
        assert_eq!(at(Mode::Binary, 5.0, 1.5 / 8.0), 0.5);
    }

    #[test]
    fn wormhole_holds_the_middle() {
        assert_eq!(at(Mode::Wormhole, 5.0, 0.5), 0.5);
        assert_eq!(at(Mode::Wormhole, 5.0, 0.45), 0.5);
        assert!((at(Mode::Wormhole, 0.0, 0.3) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn sigmoid_keeps_its_endpoints() {
        for knob_value in [0.0, 2.5, 5.0] {
            assert!(at(Mode::Sigmoid, knob_value, 0.0).abs() < 1e-5);
            assert!((at(Mode::Sigmoid, knob_value, 1.0) - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn flip_inverts_the_wave_before_the_amount() {
        let p = prepare(Mode::Flip, 2.5);
        assert_eq!(warp(Mode::Flip, 0.3, &p), (0.3, -1.0));
        assert_eq!(warp(Mode::Flip, 0.7, &p), (0.7, 1.0));
        assert_eq!(warp(Mode::Flip, 0.3, &prepare(Mode::Flip, 0.0)), (0.3, 1.0));
    }

    #[test]
    fn brownian_offset_is_deterministic_per_phase() {
        assert_eq!(at(Mode::Brownian, 3.0, 0.37), at(Mode::Brownian, 3.0, 0.37));
        assert!((at(Mode::Brownian, 0.0, 0.37) - 0.37).abs() < 1e-6);
    }
}
