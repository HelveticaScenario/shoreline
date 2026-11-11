//! Primitives shared by the `$mi.*` ports, ported from `stmlib/dsp` and
//! `plaits/dsp`.

pub mod control;
pub mod delay_line;
pub mod downsampler;
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
pub(in crate::dsp::mi) mod tables;
pub mod units;
