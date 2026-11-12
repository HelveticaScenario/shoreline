//! Plaits' two bass drums, which its bass drum engine plays side by side:
//! `$mi.kick`, the 808-style analog model through Plaits' overdrive, and
//! `$mi.synthKick`, the synthetic one.

mod analog;
mod analog_drum;
mod synthetic;
mod synthetic_drum;

pub use analog::MiKick;
pub use synthetic::MiSynthKick;

#[cfg(test)]
mod tests;
