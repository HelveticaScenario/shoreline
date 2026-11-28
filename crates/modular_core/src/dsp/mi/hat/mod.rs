//! `$mi.hiHat`: Plaits' hi-hat engine, its 808-style hat and its
//! ring-modulated variant side by side.

mod hat_drum;
mod hi_hat;
mod lane;
mod noise;

pub use hi_hat::MiHiHat;

#[cfg(test)]
mod tests;
