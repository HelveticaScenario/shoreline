//! Primitives shared by the `$mi.*` ports, ported from `stmlib/dsp` and
//! `plaits/dsp`.

pub mod control;
pub mod downsampler;
pub mod filter;
#[cfg(test)]
pub mod fixture;
pub mod interpolate;
pub mod oscillator;
pub mod phase;
pub mod polyblep;
pub mod ratio;
pub mod sine;
pub(in crate::dsp::mi) mod tables;
