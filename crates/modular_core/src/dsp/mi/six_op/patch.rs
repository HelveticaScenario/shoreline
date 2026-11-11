//! Plaits' three DX7 banks (`fm/patch.h`), unpacked once per process along
//! with everything a voice derives from a patch alone (`Voice::Setup`,
//! `OperatorEnvelope::Set`, `PitchEnvelope::Set`, `Lfo::Set`).

use std::sync::LazyLock;

use super::super::core::tables::{
    LUT_AMP_MOD_SENSITIVITY, LUT_PITCH_MOD_SENSITIVITY, SYX_BANK_0, SYX_BANK_1, SYX_BANK_2,
};
use super::algorithms::Algorithm;
use super::units::{
    frequency_ratio, lfo_delay, lfo_frequency, operator_envelope_increment, operator_level,
    pitch_envelope_increment, pitch_envelope_level,
};

pub const BANKS: usize = 3;
pub const PATCHES: usize = 32;
const SYX_SIZE: usize = 128;

/// Every patch of every bank. Unpacked on first use: modules force it in
/// `init`, on the main thread, so selecting a patch on the audio thread only
/// indexes it.
pub static PATCH_BANKS: LazyLock<Box<[[Patch; PATCHES]; BANKS]>> = LazyLock::new(|| {
    let banks: [&[u8]; BANKS] = [&SYX_BANK_0, &SYX_BANK_1, &SYX_BANK_2];
    Box::new(banks.map(|bank| {
        std::array::from_fn(|i| Patch::unpack(&bank[i * SYX_SIZE..(i + 1) * SYX_SIZE]))
    }))
});

#[derive(Clone, Copy, Debug)]
pub struct KeyboardScaling {
    pub break_point: u8,
    pub left_depth: u8,
    pub right_depth: u8,
    pub left_curve: u8,
    pub right_curve: u8,
}

/// The fields of a packed operator that set its frequency.
pub struct SyxOperator {
    pub mode: u8,
    pub coarse: u8,
    pub fine: u8,
    pub detune: u8,
}

/// A four-stage envelope's levels and per-sample increments, the increments
/// at Plaits' legacy 44.1 kHz.
#[derive(Clone, Copy, Debug)]
pub struct EnvelopeShape {
    pub level: [f32; 4],
    pub increment: [f32; 4],
}

impl EnvelopeShape {
    /// `OperatorEnvelope::Set`, before the sample-rate scale.
    fn operator(rate: [u8; 4], level: [u8; 4], global_level: i32) -> Self {
        let levels = level.map(|l| {
            let scaled = (operator_level(l) & !1) + global_level - 133;
            0.125 * if scaled < 1 { 0.5 } else { scaled as f32 }
        });
        let increment = std::array::from_fn(|i| {
            let mut increment = operator_envelope_increment(rate[i]);
            let mut from = levels[(i + 3) % 4];
            let mut to = levels[i];
            if from == to {
                // Plateaux run slower, and the attack plateau faster.
                increment *= 0.6;
                if i == 0 && level[i] == 0 {
                    increment *= 20.0;
                }
            } else if from < to {
                from = from.max(6.7);
                to = to.max(6.7);
                if from == to {
                    increment = 1.0;
                } else {
                    // The reshaped attack segment runs at this adjusted rate.
                    increment *= 7.2 / (to - from);
                }
            } else {
                increment *= 1.0 / (from - to);
            }
            increment
        });
        Self {
            level: levels,
            increment,
        }
    }

    /// `PitchEnvelope::Set`, before the sample-rate scale.
    fn pitch(rate: [u8; 4], level: [u8; 4]) -> Self {
        let levels = level.map(pitch_envelope_level);
        let increment = std::array::from_fn(|i| {
            let from = levels[(i + 3) % 4];
            let to = levels[i];
            let increment = pitch_envelope_increment(rate[i]);
            if from != to {
                increment * (1.0 / (from - to).abs())
            } else if i != 3 {
                0.2
            } else {
                increment
            }
        });
        Self {
            level: levels,
            increment,
        }
    }
}

/// An operator's settings, as its voice reads them each sample.
#[derive(Clone, Copy, Debug)]
pub struct Operator {
    pub envelope: EnvelopeShape,
    pub keyboard_scaling: KeyboardScaling,
    pub rate_scaling: u8,
    pub amp_mod_sensitivity: f32,
    pub velocity_sensitivity: f32,
    /// How far keyboard, velocity and brightness scaling may raise the level.
    pub level_headroom: f32,
    /// The frequency ratio to the note; a negative ratio is minus a fixed
    /// frequency in Hz.
    pub ratio: f32,
}

/// The LFO's settings, with its rates in Hz.
#[derive(Clone, Copy, Debug)]
pub struct LfoShape {
    pub frequency: f32,
    pub delay: [f32; 2],
    pub waveform: u8,
    pub reset_phase: bool,
    pub amp_mod_depth: f32,
    pub pitch_mod_depth: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Patch {
    /// Operators 6 down to 1, the order the algorithm renders them in.
    pub op: [Operator; 6],
    pub pitch_envelope: EnvelopeShape,
    pub algorithm: Algorithm,
    /// The feedback operator's gain on its last two outputs.
    pub feedback_scale: f32,
    pub reset_phase: bool,
    pub lfo: LfoShape,
}

impl Patch {
    /// `Patch::Unpack` of the 128-byte packed format, then each constant a
    /// voice derives from it.
    fn unpack(data: &[u8]) -> Self {
        let clamp = |byte: u8, max: u8| (byte & 0x7f).min(max);
        let op = std::array::from_fn(|i| {
            let d = &data[i * 17..(i + 1) * 17];
            let rate = std::array::from_fn(|j| clamp(d[j], 99));
            let level_bytes = std::array::from_fn(|j| clamp(d[4 + j], 99));
            let level = operator_level(clamp(d[14], 99));
            let syx = SyxOperator {
                mode: d[15] & 0x1,
                coarse: (d[15] >> 1) & 0x1f,
                fine: clamp(d[16], 99),
                detune: ((d[12] >> 3) & 0xf).min(14),
            };
            let sign = if syx.mode == 0 { 1.0 } else { -1.0 };
            Operator {
                envelope: EnvelopeShape::operator(rate, level_bytes, level),
                keyboard_scaling: KeyboardScaling {
                    break_point: clamp(d[8], 99),
                    left_depth: clamp(d[9], 99),
                    right_depth: clamp(d[10], 99),
                    left_curve: d[11] & 0x3,
                    right_curve: (d[11] >> 2) & 0x3,
                },
                rate_scaling: d[12] & 0x7,
                amp_mod_sensitivity: LUT_AMP_MOD_SENSITIVITY[usize::from(d[13] & 0x3)],
                velocity_sensitivity: f32::from((d[13] >> 2) & 0x7),
                level_headroom: (127 - level) as f32,
                ratio: sign * frequency_ratio(&syx),
            }
        });
        let pitch_rate = std::array::from_fn(|j| clamp(data[102 + j], 99));
        let pitch_level = std::array::from_fn(|j| clamp(data[106 + j], 99));
        let feedback = data[111] & 0x7;
        let sensitivity = LUT_PITCH_MOD_SENSITIVITY[usize::from(data[116] >> 4)];
        Self {
            op,
            pitch_envelope: EnvelopeShape::pitch(pitch_rate, pitch_level),
            algorithm: Algorithm::compile(usize::from(data[110] & 0x1f)),
            feedback_scale: if feedback == 0 {
                0.0
            } else {
                (1 << feedback) as f32 / 512.0
            },
            reset_phase: (data[111] >> 3) & 0x1 != 0,
            lfo: LfoShape {
                frequency: lfo_frequency(clamp(data[112], 99)),
                delay: lfo_delay(clamp(data[113], 99)),
                waveform: ((data[116] >> 1) & 0x7).min(5),
                reset_phase: data[116] & 0x1 != 0,
                amp_mod_depth: f32::from(clamp(data[115], 99)) * 0.01,
                pitch_mod_depth: f32::from(clamp(data[114], 99)) * 0.01 * sensitivity,
            },
        }
    }
}
