use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::rng::{hash_bipolar, seed_base};
use crate::dsp::utils::voct_to_hz;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PORT_MAX_CHANNELS, PolyOutput, PolySignal};

mod channel;
mod generators;
#[cfg(test)]
mod tests;

use channel::NoiseChannelState;
use generators::{LFSR_LONG_PERIOD, LFSR_SHORT_CYCLE, LfsrLoop, LfsrPhases, lfsr_even_phase};

fn default_channels() -> usize {
    1
}

/// At least `channels`, widened to match a polyphonic `rate`.
fn noise_derive_channel_count(params: &NoiseParams) -> usize {
    let rate_channels = params.rate.as_ref().map_or(0, PolySignal::channels);
    params
        .channels
        .max(rate_channels)
        .clamp(1, PORT_MAX_CHANNELS)
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
struct NoiseParams {
    #[input("kind of noise", default = "white")]
    #[serde(default)]
    #[deserr(default)]
    color: NoiseKind,
    #[input(
        "how often a new value is drawn; for telegraph and dust the average events per second, for lfsrShort the pitch of its loop. Unpatched, a new value every sample",
        type = frequency,
    )]
    #[deserr(default)]
    rate: Option<PolySignal>,
    #[input(
        "how the output moves between draws while `rate` is patched",
        default = "step"
    )]
    #[serde(default)]
    #[deserr(default)]
    interp: NoiseInterp,
    #[input(
        "number of independent noise channels, each a different sequence; a polyphonic `rate` raises it to that many",
        range = (1, 64),
    )]
    #[serde(default = "default_channels")]
    #[deserr(default = default_channels())]
    channels: usize,
}

#[derive(Clone, Copy, Deserr, JsonSchema, Debug, PartialEq, Eq, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[derive(Default)]
enum NoiseKind {
    /// equal energy across all frequencies
    #[default]
    White,
    /// rolled-off highs (−3 dB/octave), natural-sounding
    Pink,
    /// deep rumble (−6 dB/octave)
    Brown,
    /// boosted highs (+3 dB/octave)
    Blue,
    /// strongly boosted highs (+6 dB/octave), thin and airy
    Violet,
    /// only −5V or +5V, a coin flip each tick
    Binary,
    /// bell-curve values clustered around 0V (σ ≈ 1.67V)
    Gaussian,
    /// 15-bit shift-register noise (NES-style), ±5V
    Lfsr,
    /// 93-step shift-register loop: pitched, metallic, ±5V
    LfsrShort,
    /// one ±5V trigger at a random point in every tick
    Velvet,
    /// ±5V that flips sign at random moments, `rate` times per second on average
    Telegraph,
    /// rests at −5V with +5V triggers, `rate` per second on average, at
    /// loosely irregular moments with no long silences
    Dust,
}

#[derive(Clone, Copy, Deserr, JsonSchema, Debug, PartialEq, Eq, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[derive(Default)]
enum NoiseInterp {
    /// hold each value until the next tick
    #[default]
    Step,
    /// ramp straight from one value to the next
    Linear,
    /// curve through the values with no corners
    Smooth,
}

const NOISE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Noise and random source",
    description: "Hiss, rumble, crackle and random voltages from one module: \
                  colored noise for hats, wind and surf, shift-register noise \
                  for chiptune percussion, and random triggers. Clocked slowly \
                  by `rate`, any color becomes stepped or smooth random CV.",
    categories: &[ModuleCategory::Oscillator, ModuleCategory::Random],
    sections: &[],
    examples: &[
        DocExample {
            title: "Brown-noise surf",
            code: "$noise('brown').amp($sine('0.1hz').range(1, 5)).out()",
        },
        DocExample {
            title: "Random melody in key",
            code: r#"
                $sine(
                  $quantizer(
                    $noise('white', { rate: '4hz' }).range('c3', 'c5'),
                    $chord('C(minor)'),
                  ),
                ).out()
            "#,
        },
        DocExample {
            title: "Raindrop blips on random triggers",
            code: r#"
                $sine('c6')
                  .amp(
                    $perc($noise('dust', { rate: '6hz' }), { decay: 0.03 }),
                  )
                  .out()
            "#,
        },
    ],
};

#[module(
    name = "$noise", docs = NOISE_DOCS,
    channels_derive = noise_derive_channel_count,
    has_init,
    patch_update,
    args(color)
)]
pub struct Noise {
    outputs: NoiseOutputs,
    params: NoiseParams,
    state: NoiseState,
    /// Per-channel generator state, one element per polyphonic channel.
    channel_state: Box<[NoiseChannelState]>,
}

/// Module-level state for the Noise module.
#[derive(Default)]
struct NoiseState {
    last_noise_type: NoiseKind,
    inv_sample_rate: f32,
    /// Random per-module seed that places the LFSR channels, drawn when the
    /// module's first channels are seeded. It transfers with the module, so
    /// channels a patch update adds play short-mode cycles distinct from the
    /// transferred channels'.
    lfsr_seed: u32,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct NoiseOutputs {
    #[output("output", "the noise", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

impl Noise {
    fn init(&mut self, sample_rate: f32) {
        self.state.inv_sample_rate = 1.0 / sample_rate;
    }

    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();
        let color = self.params.color;

        // Unconnected, the clock ticks every sample with nothing to
        // interpolate, telegraph flips with the maximum-entropy probability
        // 1/2, and dust's gaps average two samples.
        let rate = self.params.rate.as_ref();
        let interp = if rate.is_some() {
            self.params.interp
        } else {
            NoiseInterp::Step
        };
        let ticks_per_cycle = if color == NoiseKind::LfsrShort {
            LFSR_SHORT_CYCLE as f32
        } else {
            1.0
        };

        for ch in 0..channels {
            // Rate cycles per sample, capped at one.
            let cycles = rate
                .map(|rate| (voct_to_hz(rate.get_value(ch)) * self.state.inv_sample_rate).min(1.0));
            let state = &mut self.channel_state[ch];
            let value = match color {
                NoiseKind::Telegraph => state.telegraph(cycles.unwrap_or(0.5)),
                NoiseKind::Dust => state.dust(cycles.unwrap_or(0.5), sample_rate),
                NoiseKind::Velvet => state.velvet(cycles.unwrap_or(1.0), sample_rate),
                _ => state.clocked(color, cycles.map_or(1.0, |c| c * ticks_per_cycle), interp),
            };
            self.outputs.sample.set(ch, value * 5.0);
        }
    }
}

impl crate::types::PatchUpdateHandler for Noise {
    /// Restarts every channel when the color changed, and seeds channels that
    /// have not been seeded yet: every channel of a new module, or those a
    /// patch update added beside transferred ones.
    ///
    /// Runs here, not in `init`, because `init` runs while the module still
    /// lives in a transient stack slot reused across constructions, so every
    /// instance would seed identically. By `on_patch_update` the module sits at
    /// its stable per-instance heap address. Transferred channels keep their
    /// seeded RNG, so each stream stays continuous across patch updates.
    fn on_patch_update(&mut self) {
        self.prepare_channels(seed_base(self));
    }
}

impl Noise {
    /// The body of `on_patch_update`, seeding new channels from `base`.
    fn prepare_channels(&mut self, base: u64) {
        let color = self.params.color;
        let color_changed = self.state.last_noise_type != color;
        self.state.last_noise_type = color;
        let fresh = !self.channel_state.iter().any(|state| state.seeded);
        if fresh {
            let unit = (hash_bipolar(base, 0) + 1.0) * 0.5;
            self.state.lfsr_seed = (unit * f64::from(u32::MAX)) as u32;
        }
        let lfsr_seed = self.state.lfsr_seed;
        let channels = self.channel_state.len();
        // Channels added beside running ones claim long-mode phases away from
        // the current phases of the running channels.
        let mut phases = LfsrPhases::new(
            self.channel_state
                .iter()
                .filter(|state| state.seeded)
                .map(NoiseChannelState::lfsr_phase),
        );
        for (ch, state) in self.channel_state.iter_mut().enumerate() {
            if !state.seeded {
                let phase = if fresh {
                    lfsr_even_phase(lfsr_seed % LFSR_LONG_PERIOD, ch, channels)
                } else {
                    phases.claim()
                };
                let lfsr_loop = LfsrLoop::for_channel(lfsr_seed, ch);
                state.seed(base, ch, phase, lfsr_loop, color);
            } else if color_changed {
                state.reset(color);
            }
        }
    }
}

message_handlers!(impl Noise {});
