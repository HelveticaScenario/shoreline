//! Plaits' DX7 unit conversions (`fm/dx_units.h`).

use super::super::core::{
    tables::{LUT_COARSE, LUT_CUBE_ROOT},
    units::semitones_to_ratio,
};
use super::patch::{KeyboardScaling, SyxOperator};

/// `Pow2Fast<1>`: 2^`x` from the float's bit layout, exact at integers and
/// linear between them.
#[inline(always)]
pub fn pow2_fast_1(x: f32) -> f32 {
    f32::from_bits(((1 << 23) as f32 * (127.0 + x)) as i32 as u32)
}

/// `Pow2Fast<2>`: 2^`x` with a quadratic between integers, for `x` above −126.
#[inline(always)]
pub fn pow2_fast_2(x: f32) -> f32 {
    let mut integral = x as i32;
    if x < 0.0 {
        integral -= 1;
    }
    let x = x - integral as f32;
    let r = 1.0 + x * (0.6565 + x * 0.3435);
    f32::from_bits((r.to_bits() as i32).wrapping_add(integral << 23) as u32)
}

pub fn operator_level(level: u8) -> i32 {
    let tlc = i32::from(level);
    if level < 20 {
        if tlc < 15 {
            (tlc * (36 - tlc)) >> 3
        } else {
            27 + tlc
        }
    } else {
        tlc + 28
    }
}

pub fn pitch_envelope_level(level: u8) -> f32 {
    let l = (f32::from(level) - 50.0) / 32.0;
    let tail = ((l + 0.02).abs() - 1.0).max(0.0);
    l * (1.0 + tail * tail * 5.3056)
}

pub fn operator_envelope_increment(rate: u8) -> f32 {
    let rate_scaled = (i32::from(rate) * 41) >> 6;
    let mantissa = 4 + (rate_scaled & 3);
    let exponent = 2 + (rate_scaled >> 2);
    (mantissa << exponent) as f32 / (1 << 24) as f32
}

pub fn pitch_envelope_increment(rate: u8) -> f32 {
    let r = f32::from(rate) * 0.01;
    (1.0 + 192.0 * r * (r * r * r * r + 0.3333)) / (21.3 * 44100.0)
}

const MIN_LFO_FREQUENCY: f32 = 0.005865;

/// The LFO's frequency in Hz.
pub fn lfo_frequency(rate: u8) -> f32 {
    let mut rate_scaled = if rate == 0 {
        1
    } else {
        (i32::from(rate) * 165) >> 6
    };
    rate_scaled *= if rate_scaled < 160 {
        11
    } else {
        11 + ((rate_scaled - 160) >> 4)
    };
    rate_scaled as f32 * MIN_LFO_FREQUENCY
}

/// The LFO delay's two ramp rates, in Hz.
pub fn lfo_delay(delay: u8) -> [f32; 2] {
    if delay == 0 {
        [100000.0; 2]
    } else {
        let d = 99 - i32::from(delay);
        let d = (16 + (d & 15)) << (1 + (d >> 4));
        [
            d as f32 * MIN_LFO_FREQUENCY,
            0x80.max(d & 0xff80) as f32 * MIN_LFO_FREQUENCY,
        ]
    }
}

/// Velocity 0–1 as the level offset the operators' sensitivities scale.
pub fn normalize_velocity(velocity: f32) -> f32 {
    let index = velocity * 16.0;
    let integral = index as i32;
    let fractional = index - integral as f32;
    let a = LUT_CUBE_ROOT[integral.clamp(0, 16) as usize];
    let b = LUT_CUBE_ROOT[(integral + 1).clamp(0, 16) as usize];
    16.0 * ((a + (b - a) * fractional) - 0.918)
}

#[inline(always)]
pub fn rate_scaling(note: f32, rate_scaling: u8) -> f32 {
    pow2_fast_1(f32::from(rate_scaling) * (note * 0.33333 - 7.0) * 0.03125)
}

pub fn keyboard_scaling(note: f32, ks: &KeyboardScaling) -> f32 {
    let x = note - f32::from(ks.break_point) - 15.0;
    let curve = if x > 0.0 {
        ks.right_curve
    } else {
        ks.left_curve
    };
    let mut t = x.abs();
    if curve == 1 || curve == 2 {
        t = (t * 0.010467).min(1.0);
        t = t * t * t;
        t *= 96.0;
    }
    if curve < 2 {
        t = -t;
    }
    let depth = f32::from(if x > 0.0 {
        ks.right_depth
    } else {
        ks.left_depth
    });
    t * depth * 0.02677
}

/// `stmlib::SemitonesToRatioSafe`: [`semitones_to_ratio`] past ±120
/// semitones.
fn semitones_to_ratio_safe(mut semitones: f32) -> f32 {
    let mut scale = 1.0;
    while semitones > 120.0 {
        semitones -= 120.0;
        scale *= 1024.0;
    }
    while semitones < -120.0 {
        semitones += 120.0;
        scale *= 1.0 / 1024.0;
    }
    scale * semitones_to_ratio(semitones)
}

/// The operator's frequency ratio to the note, or its fixed frequency in Hz.
pub fn frequency_ratio(op: &SyxOperator) -> f32 {
    let detune = if op.mode == 0 && op.fine != 0 {
        1.0 + 0.01 * f32::from(op.fine)
    } else {
        1.0
    };
    let mut base = if op.mode == 0 {
        LUT_COARSE[usize::from(op.coarse)]
    } else {
        f32::from(u16::from(op.coarse & 3) * 100 + u16::from(op.fine)) * 0.39864
    };
    base += (f32::from(op.detune) - 7.0) * 0.015;
    semitones_to_ratio_safe(base) * detune
}
