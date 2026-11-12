//! Plaits' two snare drums, which its snare drum engine plays side by side:
//! `$mi.snare`, the 808-style analog model, and `$mi.synthSnare`, the
//! synthetic one.

mod analog;
mod analog_drum;
mod synthetic;
mod synthetic_drum;

pub use analog::MiSnare;
pub use synthetic::MiSynthSnare;

#[cfg(test)]
mod tests;
