//! Plaits' two hi-hats, which its hi-hat engine plays side by side:
//! `$mi.hiHat`, the 808-style model, and `$mi.ringHat`, its ring-modulated
//! variant.

mod channel;
mod hat_drum;
mod hi_hat;
mod noise;
mod ring_hat;

pub use hi_hat::MiHiHat;
pub use ring_hat::MiRingHat;

#[cfg(test)]
mod tests;
