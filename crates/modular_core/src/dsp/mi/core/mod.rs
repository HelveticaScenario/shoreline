//! Primitives shared by the `$mi.*` ports, ported from `stmlib/dsp` and
//! `plaits/dsp`.

pub mod blend;
pub mod chords;
pub mod control;
pub mod delay_line;
pub mod downsampler;
pub mod dsp;
pub mod ensemble;
pub mod fast_sine;
pub mod filter;
#[cfg(test)]
pub mod fixture;
pub mod interpolate;
pub mod limiter;
pub mod lpc;
pub mod oscillator;
pub mod phase;
pub mod polyblep;
pub mod random;
pub mod ratio;
pub mod sine;
pub mod tables;
pub mod units;
pub mod variable_saw;
pub mod variable_shape;
