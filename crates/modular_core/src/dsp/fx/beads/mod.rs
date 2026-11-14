//! Port of Mutable Instruments Beads (beads/dsp, copyright 2017 Emilie
//! Gillet, MIT licence), split into modules: grain voices (`$granular`,
//! `$granularSampler`), the delay mode (`$clockedDelay`), wow and flutter
//! (`$wowFlutter`) and the reverb with its limiters (`$reverb`). Grains start
//! on an external trigger.

mod buffer;
mod chain;
mod clocked_delay;
mod delay_chain;
mod delay_controller;
mod delay_windows;
mod feedback;
mod flutter;
mod fresh;
mod grain;
mod grains;
mod grains_chain;
mod granular;
mod multitap;
mod player;
mod primitives;
mod queue;
mod resample;
mod reverb;
mod sampler;
mod verb;
mod voice;
mod wav_load;
mod wow_flutter;

#[cfg(test)]
mod tests;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::sanitize;
use crate::poly::{PolySignal, PolySignalExt};

use chain::Media;
pub use clocked_delay::ClockedDelay;
pub use flutter::WowFlutter;
pub use grains::Granular;
use player::GrainParams;
pub use sampler::GranularSampler;
pub use verb::BeadsReverb;

/// Volts per hardware full scale.
const FULL_SCALE_VOLTS: f32 = 5.0;

/// Sound character, after Beads' four quality settings. Lower qualities
/// sound darker and grittier and hold more audio.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
pub enum GranularQuality {
    /// clean and clear
    #[default]
    Digital,
    /// slightly gritty, like early digital samplers
    Vintage,
    /// darker and softer, like clean tape
    CleanTape,
    /// lo-fi and noisy, like worn tape
    DirtyTape,
}

impl GranularQuality {
    fn media(self) -> Media {
        match self {
            GranularQuality::Digital => Media::Digital,
            GranularQuality::Vintage => Media::VintageDigital,
            GranularQuality::CleanTape => Media::CleanTape,
            GranularQuality::DirtyTape => Media::DirtyTape,
        }
    }
}

/// Map a 0–5 V control to the hardware's 0–1 range.
#[inline]
fn unit(v: f32) -> f32 {
    (v / FULL_SCALE_VOLTS).clamp(0.0, 1.0)
}

/// Unspread grain controls of `voice`, read from the grain inputs in volts.
fn grain_params(
    time: &Option<PolySignal>,
    size: &Option<PolySignal>,
    shape: &Option<PolySignal>,
    freq: &Option<PolySignal>,
    root_freq: &Option<PolySignal>,
    voice: usize,
) -> GrainParams {
    let freq = freq.value_or(voice, 0.0);
    let root = root_freq.value_or(voice, 0.0);
    GrainParams {
        time: unit(time.value_or(voice, 0.0)),
        size: (sanitize(size.value_or(voice, 2.75)) / FULL_SCALE_VOLTS).clamp(-1.0, 1.0),
        shape: unit(shape.value_or(voice, 3.33)),
        pitch: sanitize((freq - root) * 12.0).clamp(-84.0, 84.0),
        spread: 0.0,
    }
}

/// Lane `lane` of an audio input in hardware units.
#[inline]
fn input_value(input: &PolySignal, lane: usize) -> f32 {
    sanitize(input.get_value(lane)) * (1.0 / FULL_SCALE_VOLTS)
}
