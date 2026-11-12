//! Port of the drum-map mode of Mutable Instruments Grids
//! (grids/pattern_generator.cc, copyright 2011 Emilie Gillet, GPL-3.0-or-later).

mod nodes;

use deserr::{DeserializeError, Deserr, ErrorKind, IntoValue, ValuePointerRef};
use napi::Result;
use schemars::JsonSchema;

use crate::MonoSignal;
use crate::dsp::seq::FRESH_WINDOW_SECS;
use crate::dsp::seq::groove::{GrooveHold, before_first_step, fill_groove_inputs};
use crate::dsp::utils::rng::{LcgRng, seed_base};
use crate::dsp::utils::{GATE_HIGH_VOLTAGE, TempGate, TempGateState, min_gate_samples};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::param_errors::ModuleParamErrors;
use crate::poly::{MonoSignalExt, PolyOutput, PolySignal, PolySignalExt};
use crate::types::ClockMessages;
use crate::types::PatchUpdateHandler;

use nodes::NODES;

const NUM_PARTS: usize = 3;
const STEPS_PER_PATTERN: usize = 32;
/// Map-level threshold above which a hit also fires its accent output.
const ACCENT_LEVEL: u8 = 192;

/// Node index at each corner of the 5×5 map, indexed `[x][y]`.
const DRUM_MAP: [[usize; 5]; 5] = [
    [10, 8, 0, 9, 11],
    [15, 7, 13, 12, 6],
    [18, 14, 4, 5, 3],
    [23, 16, 21, 1, 2],
    [24, 19, 17, 20, 22],
];

/// Steps per cycle, written in the DSL as the number `16` or `32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Connect, Default)]
enum DrumMapResolution {
    #[default]
    Sixteenth,
    ThirtySecond,
}

impl<E: DeserializeError> Deserr<E> for DrumMapResolution {
    fn deserialize_from_value<V: IntoValue>(
        value: deserr::Value<V>,
        location: ValuePointerRef<'_>,
    ) -> std::result::Result<Self, E> {
        match value {
            deserr::Value::Integer(16) => Ok(Self::Sixteenth),
            deserr::Value::Integer(32) => Ok(Self::ThirtySecond),
            deserr::Value::Float(16.0) => Ok(Self::Sixteenth),
            deserr::Value::Float(32.0) => Ok(Self::ThirtySecond),
            _ => Err(deserr::take_cf_content(E::error::<V>(
                None,
                ErrorKind::Unexpected {
                    msg: "resolution must be 16 or 32".to_string(),
                },
                location,
            ))),
        }
    }
}

impl JsonSchema for DrumMapResolution {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("DrumMapResolution")
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        serde_json::from_value(serde_json::json!({
            "oneOf": [
                {
                    "const": 16,
                    "description": "16 steps per cycle, playing the map's even steps (Grids at 4 ppqn)"
                },
                {
                    "const": 32,
                    "description": "all 32 steps per cycle, including the map's ghost notes (Grids at 8 or 24 ppqn)"
                }
            ]
        }))
        .expect("DrumMapResolution schema is valid JSON Schema")
    }
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields, validate = drum_map_fill_groove -> ModuleParamErrors)]
struct DrumMapParams {
    #[input(
        "playhead position in cycles; one cycle plays the 32-step pattern once; follows the global clock by default",
        type = control,
        range = (0.0, ..),
        control_range = (0.0, 1.0),
    )]
    #[default_connection(module = RootClock, port = "playhead", channel = 0)]
    #[deserr(default)]
    playhead: Option<MonoSignal>,
    #[input(
        "groove shift of each step, in cycles, e.g. from `$groove`; 0 plays straight. Follows the global groove, unless you set your own `playhead`: then it plays straight until given a groove too",
        type = control,
        control_range = (-5.0, 5.0),
    )]
    #[default_connection(module = RootClock, port = "grooveOffset", channel = 0)]
    #[deserr(default)]
    groove: Option<MonoSignal>,
    #[input(
        "how readily each step accents: lower accents less, 0 never. Follows the global groove, unless you set your own `playhead` or `groove`: then it is full until given a velocity too",
        type = control,
        range = (0.0, ..),
        control_range = (0.0, 5.0),
    )]
    #[default_connection(module = RootClock, port = "grooveVelocity", channel = 0)]
    #[deserr(default)]
    velocity: Option<MonoSignal>,
    #[input(
        "horizontal position in the map of drum patterns",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    x: Option<PolySignal>,
    #[input(
        "vertical position in the map of drum patterns",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    y: Option<PolySignal>,
    #[input(
        "random variation of the pattern, re-rolled each cycle",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    chaos: Option<PolySignal>,
    #[input("how many bass drum hits play: none at 0, all at 5", type = control, default = 2.5, range = (0.0, 5.0))]
    #[deserr(default)]
    bd_density: Option<PolySignal>,
    #[input("how many snare drum hits play: none at 0, all at 5", type = control, default = 2.5, range = (0.0, 5.0))]
    #[deserr(default)]
    sd_density: Option<PolySignal>,
    #[input("how many hi-hat hits play: none at 0, all at 5", type = control, default = 2.5, range = (0.0, 5.0))]
    #[deserr(default)]
    hh_density: Option<PolySignal>,
    #[input("steps per cycle; 32 adds ghost notes", default = 16)]
    #[serde(default)]
    #[deserr(default)]
    resolution: DrumMapResolution,
}

fn drum_map_fill_groove(
    mut params: DrumMapParams,
    _location: ValuePointerRef,
) -> std::result::Result<DrumMapParams, ModuleParamErrors> {
    fill_groove_inputs(&params.playhead, &mut params.groove, &mut params.velocity);
    Ok(params)
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct DrumMapOutputs {
    #[output("bd", "bass drum", type = trig, default)]
    bd: PolyOutput,
    #[output("sd", "snare drum", type = trig)]
    sd: PolyOutput,
    #[output("hh", "hi-hat", type = trig)]
    hh: PolyOutput,
    #[output("bdAccent", "bass drum accent", type = trig)]
    bd_accent: PolyOutput,
    #[output("sdAccent", "snare drum accent", type = trig)]
    sd_accent: PolyOutput,
    #[output("hhAccent", "hi-hat accent", type = trig)]
    hh_accent: PolyOutput,
}

/// What the next update does with the step it lands on.
#[derive(Default, Clone, Copy, PartialEq)]
enum Sync {
    /// A fresh module's first sample: record the step and its position.
    #[default]
    Fresh,
    /// A fresh module's second sample: the playhead's speed reveals how long
    /// before creation the recorded step began, and the step plays one sample
    /// late if that is within [`FRESH_WINDOW_SECS`].
    Probing {
        /// Position within the cycle, in steps, on the first sample.
        position: f64,
    },
    /// Record the step without firing: a stopped transport stays silent until
    /// the next step boundary.
    Silent,
    /// Fire the step and start a new cycle: the transport just started, so the
    /// first sample lands in the downbeat.
    Restart,
    /// Fire on forward step changes, measured from `step`.
    Tracking,
}

#[derive(Default)]
struct DrumMapState {
    sync: Sync,
    /// Most recent step passed within the cycle, 0–31.
    step: usize,
    /// Keeps the grooved position from running backwards when the groove
    /// changes mid-play.
    groove_hold: GrooveHold,
}

#[derive(Default, Clone, Copy)]
struct ChannelState {
    rng: LcgRng,
    seeded: bool,
    perturbation: [u8; NUM_PARTS],
    triggers: [TempGate; NUM_PARTS],
    accents: [TempGate; NUM_PARTS],
}

const DRUM_MAP_DOCS: ModuleDocs = ModuleDocs {
    summary: "Topographic drum pattern generator",
    description: "Bass drum, snare and hi-hat triggers read from a map of drum patterns, \
                  modelled on Mutable Instruments Grids. Move `x` and `y` to morph between \
                  grooves, turn up each part's density for busier fills, and add `chaos` for \
                  variation; strong hits also fire the matching accent output.",
    categories: &[ModuleCategory::Sequencer, ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Morphing drum loop",
        code: r#"
            const drums = $drumMap({
              x: $sine('0.05hz').range(0, 5),
              y: 3,
              chaos: 0.5,
            })
            $tr808.kick(drums).out()
            $tr808.snare(drums._sd).out()
            $tr808.hiHat(drums._hh).out()
        "#,
    }],
};

#[module(name = "$drumMap", docs = DRUM_MAP_DOCS, args(), patch_update)]
pub struct DrumMap {
    outputs: DrumMapOutputs,
    params: DrumMapParams,
    state: DrumMapState,
    channel_state: Box<[ChannelState]>,
}

message_handlers!(impl DrumMap {
    Clock(m) => DrumMap::on_clock_message,
});

/// Maps a 0–5 V knob voltage onto Grids' 8-bit parameter range.
fn knob(volts: f32) -> u8 {
    (volts * (255.0 / 5.0)).round().clamp(0.0, 255.0) as u8
}

fn u8_mix(a: u8, b: u8, balance: u8) -> u8 {
    ((a as u16 * (255 - balance) as u16 + b as u16 * balance as u16) >> 8) as u8
}

fn u8_mul_shift8(a: u8, b: u8) -> u8 {
    ((a as u16 * b as u16) >> 8) as u8
}

/// Hit level of `part` at `step`, bilinearly interpolated between the four
/// map nodes surrounding `(x, y)`.
fn read_drum_map(step: usize, part: usize, x: u8, y: u8) -> u8 {
    let i = (x >> 6) as usize;
    let j = (y >> 6) as usize;
    let offset = part * STEPS_PER_PATTERN + step;
    let a = NODES[DRUM_MAP[i][j]][offset];
    let b = NODES[DRUM_MAP[i + 1][j]][offset];
    let c = NODES[DRUM_MAP[i][j + 1]][offset];
    let d = NODES[DRUM_MAP[i + 1][j + 1]][offset];
    u8_mix(u8_mix(a, b, x << 2), u8_mix(c, d, x << 2), y << 2)
}

impl DrumMap {
    fn on_clock_message(&mut self, m: &ClockMessages) -> Result<()> {
        self.state.sync = match m {
            ClockMessages::Start => Sync::Restart,
            ClockMessages::Stop => Sync::Silent,
        };
        Ok(())
    }

    fn update(&mut self, sample_rate: f32) {
        // Summed in f64: the clock's bar count grows without bound, and an f32
        // sum would erode the fractional bar phase that selects the step.
        let straight = self
            .params
            .playhead
            .as_ref()
            .map_or(0.0, MonoSignal::get_value_f64);
        let playhead = self.state.groove_hold.apply(
            straight,
            straight + self.params.groove.value_or_zero() as f64,
        );
        // Before a late first step nothing plays, and a pending transport
        // start fires once that step arrives.
        if !before_first_step(straight, playhead) {
            self.advance_to(playhead, sample_rate);
        }

        for ch in 0..self.channel_count() {
            let state = &mut self.channel_state[ch];
            self.outputs.bd.set(ch, state.triggers[0].process());
            self.outputs.sd.set(ch, state.triggers[1].process());
            self.outputs.hh.set(ch, state.triggers[2].process());
            self.outputs.bd_accent.set(ch, state.accents[0].process());
            self.outputs.sd_accent.set(ch, state.accents[1].process());
            self.outputs.hh_accent.set(ch, state.accents[2].process());
        }
    }

    /// Move to grooved position `playhead` (cycles), playing the step it
    /// reaches when that step fires.
    fn advance_to(&mut self, playhead: f64, sample_rate: f32) {
        let position = (playhead - playhead.floor()) * STEPS_PER_PATTERN as f64;
        let stride = match self.params.resolution {
            DrumMapResolution::Sixteenth => 2,
            DrumMapResolution::ThirtySecond => 1,
        };
        let step = (position as usize).min(STEPS_PER_PATTERN - 1) / stride * stride;

        let last = self.state.step;
        let tracking = matches!(self.state.sync, Sync::Tracking | Sync::Probing { .. });
        // A backward jump over more than half the cycle is a wrap into the next
        // cycle (the clock's own bar-boundary rule). A rewind of at most one
        // step is jitter, such as a Link resync or a resolution change, and
        // keeps the previous step so it is not replayed; a longer rewind is a
        // seek and re-syncs silently.
        let (fire, new_cycle) = match self.state.sync {
            Sync::Fresh | Sync::Silent => (false, false),
            Sync::Restart => (true, true),
            Sync::Probing { position: first } if step == last => {
                let steps_per_sample = (position - first).rem_euclid(STEPS_PER_PATTERN as f64);
                let window = steps_per_sample * FRESH_WINDOW_SECS * sample_rate as f64;
                let on_boundary = steps_per_sample > 0.0 && first - last as f64 <= window;
                (on_boundary, on_boundary && step == 0)
            }
            _ if tracking && step > last => (true, false),
            _ if tracking && last - step > STEPS_PER_PATTERN / 2 => (true, true),
            _ => (false, false),
        };
        let jitter = tracking && step < last && last - step <= stride;
        if !jitter {
            self.state.step = step;
        }
        self.state.sync = match self.state.sync {
            Sync::Fresh => Sync::Probing { position },
            _ => Sync::Tracking,
        };

        if fire {
            let velocity = self.params.velocity.value_or(GATE_HIGH_VOLTAGE) / GATE_HIGH_VOLTAGE;
            self.evaluate(step, new_cycle, velocity, min_gate_samples(sample_rate));
        }
    }

    /// Play `step`. `velocity` (0–1) scales each hit's level against the
    /// accent threshold.
    fn evaluate(&mut self, step: usize, new_cycle: bool, velocity: f32, hold: u32) {
        let p = &self.params;
        for ch in 0..self.channel_count() {
            let state = &mut self.channel_state[ch];
            if new_cycle {
                let randomness = knob(p.chaos.value_or(ch, 0.0)) >> 2;
                for perturbation in state.perturbation.iter_mut() {
                    let byte = (state.rng.next_unit() * 256.0) as u8;
                    *perturbation = u8_mul_shift8(byte, randomness);
                }
            }

            let x = knob(p.x.value_or(ch, 2.5));
            let y = knob(p.y.value_or(ch, 2.5));
            let densities = [
                knob(p.bd_density.value_or(ch, 2.5)),
                knob(p.sd_density.value_or(ch, 2.5)),
                knob(p.hh_density.value_or(ch, 2.5)),
            ];
            for (part, &density) in densities.iter().enumerate() {
                let level =
                    read_drum_map(step, part, x, y).saturating_add(state.perturbation[part]);
                if level > !density {
                    state.triggers[part].set_state(TempGateState::High, TempGateState::Low, hold);
                    if level as f32 * velocity > ACCENT_LEVEL as f32 {
                        state.accents[part].set_state(
                            TempGateState::High,
                            TempGateState::Low,
                            hold,
                        );
                    }
                }
            }
        }
    }
}

impl PatchUpdateHandler for DrumMap {
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, state) in self.channel_state.iter_mut().enumerate() {
            if !state.seeded {
                state.rng.seed(base, ch);
                state.seeded = true;
            }
        }
    }
}

#[cfg(test)]
mod tests;
