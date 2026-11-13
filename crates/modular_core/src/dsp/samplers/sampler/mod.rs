use deserr::Deserr;
use schemars::JsonSchema;

use self::voice::{RELEASE_SLOTS, Ramp, Release, Voice, push_release};
use super::slice::SliceParam;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, DocSection, ModuleDocs};
use crate::{
    Wav,
    dsp::utils::SchmittTrigger,
    param_errors::ModuleParamErrors,
    poly::{LaneSum, LaneSumExt, PORT_MAX_LANES, PolyOutput},
};

mod looping;
mod voice;

/// Output width follows the WAV, capped at the port-wide lane limit — a
/// WAV header can claim any u16 channel count.
fn sampler_derive_lane_count(params: &SamplerParams) -> usize {
    params.wav.channel_count().clamp(1, PORT_MAX_LANES)
}

fn default_fade() -> f64 {
    1.0
}

/// Upper bound on the declick fade time.
const MAX_FADE_MS: f64 = 1000.0;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields, validate = sampler_validate_params -> ModuleParamErrors)]
struct SamplerParams {
    #[input("the WAV file to play")]
    wav: Wav,
    #[input(
        "starts the selected slice playing; `mode` sets what the gate does after that",
        type = gate,
    )]
    gate: LaneSum,
    #[input(
        "playback speed: 1 normal, 2 double (an octave up), negative plays in reverse",
        type = control,
        default = 1.0,
        control_range = (-4.0, 4.0),
    )]
    #[deserr(default)]
    speed: Option<LaneSum>,
    #[input(
        "which slice plays, read at each gate and each loop repeat: a signal, or `[points, signal]` to cut the file into slices (see Slicing)"
    )]
    #[deserr(default)]
    slice: Option<SliceParam>,
    #[input(
        "declick fade in milliseconds, at the start and end of each pass, on release and across retriggers; 0 turns it off",
        range = (0.0, 1000.0)
    )]
    #[serde(default = "default_fade")]
    #[deserr(default = default_fade())]
    fade: f64,
    #[input("what the gate does once playback starts", default = "once")]
    #[serde(default)]
    #[deserr(default)]
    mode: SamplerMode,
}

/// How the gate controls playback.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, JsonSchema, Connect)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
enum SamplerMode {
    /// each gate plays the slice through; the gate falling does nothing
    #[default]
    Once,
    /// the slice repeats in time while the gate is high, picking the slice
    /// again at each repeat, and fades out over `fade` when the gate falls
    Loop,
    /// the slice plays while the gate is high and fades out over `fade` when
    /// it falls
    Gate,
}

impl SamplerMode {
    fn loops(self) -> bool {
        self == SamplerMode::Loop
    }

    /// Playback stops while the gate is low.
    fn follows_gate(self) -> bool {
        matches!(self, SamplerMode::Loop | SamplerMode::Gate)
    }
}

fn sampler_validate_params(
    params: SamplerParams,
    _location: deserr::ValuePointerRef,
) -> Result<SamplerParams, ModuleParamErrors> {
    if !params.fade.is_finite() || params.fade < 0.0 || params.fade > MAX_FADE_MS {
        let mut err = ModuleParamErrors::default();
        err.add(
            "fade".to_string(),
            format!("fade must be between 0 and {MAX_FADE_MS} milliseconds"),
        );
        return Err(err);
    }
    Ok(params)
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SamplerOutputs {
    #[output("output", "the played sample", type = cv, default)]
    sample: PolyOutput,
}

#[derive(Default)]
struct SamplerState {
    main: Voice,
    playing: bool,
    gate_trigger: SchmittTrigger,
    /// Engine sample rate, captured in init.
    sample_rate: f32,
    /// wav_rate / engine_rate. The WAV's sample rate only resolves after
    /// connect(), so this is computed in on_patch_update, not init. Constant
    /// until the next patch update.
    rate_ratio: f64,
    /// The current pass's window in WAV frames, picked when the pass
    /// starts. A playing voice keeps it across patch updates.
    win_lo: f64,
    win_hi: f64,
    /// The next loop pass's window, picked as the seam comes in reach.
    next_win: Option<(f64, f64)>,
    /// Declick ramp width in WAV frames (fade seconds × wav rate).
    /// Param+wav-derived, so computed in on_patch_update; 0 disables fades.
    fade_frames: f64,
    /// Timed release-voice ramp length in output samples (fade seconds ×
    /// engine rate). Param-derived, so computed in on_patch_update.
    fade_out_samples: u32,
    rels: [Release; RELEASE_SLOTS],
    /// The main pass ends at its window edge instead of repeating: always
    /// outside loop mode, and for a loop pass once the mode changes or the
    /// next slice is empty. Its closing fade is set when this becomes true. A
    /// gate still holding a loop open starts a new pass once this one is
    /// done.
    playing_out: bool,
}

const SAMPLER_DOCS: ModuleDocs = ModuleDocs {
    summary: "Gate-triggered sample player",
    description: "Plays a WAV file, or one slice of it, each time the gate rises: one-shots, \
                  held notes or loops depending on `mode`. Change `speed` to pitch it up, down \
                  or into reverse, and step `slice` through a pattern to chop a break.",
    categories: &[ModuleCategory::Sampler],
    sections: &[DocSection {
        title: "Slicing",
        body: "Give `slice` a signal and the whole file is one slice. Give it `[points, signal]` \
               to cut the file, each point a slice start as a fraction of the file from 0 to 1. \
               Either way the signal's whole part picks the slice, wrapping past the last, and \
               its fraction starts playback partway into it.",
        table: None,
        examples: &[],
    }],
    examples: &[
        DocExample {
            title: "Chopped and rearranged loop",
            code: r#"
                const order = $p('0 2 1 3 0 0 3 2')
                $sampler('/wavs/pad.wav', order._trig, {
                  slice: [[0, 0.25, 0.5, 0.75], order],
                  fade: 2,
                }).out()
            "#,
        },
        DocExample {
            title: "Kick on every beat",
            code: "$sampler('/wavs/kick.wav', $clock._beatTrigger).out()",
        },
    ],
};

#[module(name = "$sampler", docs = SAMPLER_DOCS, lanes_derive = sampler_derive_lane_count, args(wav, gate), has_init, patch_update)]
pub struct Sampler {
    params: SamplerParams,
    outputs: SamplerOutputs,
    state: SamplerState,
}

impl Sampler {
    /// Capture the engine sample rate. Invoked by the `#[module]` proc macro at
    /// construction. The playback rate ratio is finished in on_patch_update,
    /// once the WAV is connected.
    fn init(&mut self, sample_rate: f32) {
        self.state.sample_rate = sample_rate;
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let frame_count = self.params.wav.frame_count();

        if !self.params.wav.is_loaded() || frame_count == 0 {
            for lane in 0..lanes {
                self.outputs.sample.set(lane, 0.0);
            }
            return;
        }
        let max_frame = (frame_count - 1) as f64;

        let gate_val = self.params.gate.get_value();
        let (gate_high, edge) = self.state.gate_trigger.process_with_edge(gate_val);
        let speed = self.params.speed.value_or(1.0) as f64;
        let advance = speed * self.state.rate_ratio;
        // A retrigger releases the voice still playing so the jump to the
        // new onset crossfades instead of clicking, and a mode that follows
        // the gate releases it while the gate is low. `playing` lags the
        // position by one sample (the span-exit check runs after edge
        // detection), so also require the position to still be in the span:
        // an exited voice is silent and must not displace a live release.
        let retrigger = edge.is_rising();
        let gate_off = !gate_high && self.params.mode.follows_gate();
        if (retrigger || gate_off) && self.state.playing && self.state.main.in_span() {
            self.release(advance, max_frame, retrigger);
        }
        if retrigger {
            let (lo, hi) = self.window(frame_count, max_frame);
            self.start_pass(lo, hi, speed, max_frame);
        }

        let holding = self.params.mode.loops() && gate_high;
        if self.state.playing && !self.state.playing_out {
            if holding {
                self.loop_seam(advance, frame_count, max_frame);
            } else {
                self.end_loop(advance, max_frame);
            }
        } else if holding && !self.state.playing {
            // An empty slice has nothing to play; the loop picks up as soon
            // as the slice signal selects one that does.
            let (lo, hi) = self.window(frame_count, max_frame);
            if hi > lo {
                self.start_pass(lo, hi, speed, max_frame);
            }
        }

        let fade = self.state.fade_frames;
        let mut main_gain = 0.0;
        if self.state.playing {
            if self.state.main.in_span() {
                main_gain = self.state.main.gain(fade);
            } else {
                self.state.playing = false;
            }
        }

        let fade_out_samples = self.state.fade_out_samples;
        let mut rel_gains = [0.0; RELEASE_SLOTS];
        for (gain, rel) in rel_gains.iter_mut().zip(&mut self.state.rels) {
            *gain = rel.gain(fade, fade_out_samples);
        }
        let any_rel = self.state.rels.iter().any(|r| r.active);

        if !self.state.playing && !any_rel {
            for lane in 0..lanes {
                self.outputs.sample.set(lane, 0.0);
            }
            return;
        }

        let wav = &self.params.wav;
        for lane in 0..lanes {
            let samples = wav.channel(lane);
            let mut value = 0.0;
            if self.state.playing {
                // Expected range of wav is -1.0 to 1.0, multiply by 5 to get to oscillator level
                value += self.state.main.read(samples) * 5.0 * main_gain;
            }
            for (rel, &gain) in self.state.rels.iter().zip(&rel_gains) {
                if rel.active {
                    value += rel.voice.read(samples) * 5.0 * gain;
                }
            }
            self.outputs.sample.set(lane, value);
        }

        // Advance positions, compensating for sample rate difference (ratio
        // resolved in on_patch_update).
        if self.state.playing {
            self.state.main.position += advance;
        }
        for rel in &mut self.state.rels {
            rel.advance(advance, fade_out_samples);
        }
    }

    /// Start a fresh main pass over `[lo, hi]`. Reverse playback enters
    /// through the window's far edge: the same audible region, played
    /// backwards.
    fn start_pass(&mut self, lo: f64, hi: f64, speed: f64, max_frame: f64) {
        self.state.win_lo = lo;
        self.state.win_hi = hi;
        self.state.next_win = None;
        self.state.main = Voice {
            position: if speed < 0.0 { hi } else { lo },
            lo,
            hi,
            lo_ramp: Ramp::Fade,
            hi_ramp: Ramp::Fade,
            wrap_reads: hi > max_frame,
        };
        // Both edges fade by default. A loop pass sounds at full gain up to
        // the edge it heads for instead: the seam's crossfade, or the
        // play-out fade, sets that ramp.
        if self.params.mode.loops() {
            if speed < 0.0 {
                self.state.main.lo_ramp = Ramp::Frames(0.0);
            } else {
                self.state.main.hi_ramp = Ramp::Frames(0.0);
            }
        }
        self.state.playing = true;
        self.state.playing_out = !self.params.mode.loops();
    }

    /// The main pass becomes a release voice that fades out over the fade
    /// time, or sooner at the window edge of a pass already playing out, so
    /// a retrigger crossfades with it. A silent voice has nothing to release
    /// and must not displace a live release, and without a fade the pass
    /// stops dead.
    ///
    /// A retrigger release fades by its span ramp, which mirrors the new
    /// pass's fade-in at any speed so the crossfade never dips; a gate
    /// release is timed, so it ends within the fade time however the speed
    /// changes.
    fn release(&mut self, advance: f64, max_frame: f64, retrigger: bool) {
        self.state.playing = false;
        let mut voice = self.state.main;
        let fade = self.state.fade_frames;
        let gain = voice.gain(fade);
        if gain <= 0.0 || fade <= 0.0 {
            return;
        }
        voice.fade_out(advance >= 0.0, fade, max_frame, self.state.playing_out);
        let release = if retrigger {
            Release::new(voice, advance)
        } else {
            Release::timed(voice, gain, self.state.fade_out_samples)
        };
        push_release(&mut self.state.rels, release);
    }

    /// The playback window in WAV frames for the slice signal's current
    /// value, or the whole file without a slice.
    fn window(&self, frame_count: usize, max_frame: f64) -> (f64, f64) {
        match &self.params.slice {
            Some(slice) => {
                // A cable can carry non-finite values; fall back to the first
                // slice so the window and position stay finite.
                let s = slice.signal.get_value_f64();
                let s = if s.is_finite() { s } else { 0.0 };
                let n = slice.points.len();
                let idx = (s.floor() as i64).rem_euclid(n as i64) as usize;
                let frac = s - s.floor();
                let start_p = slice.points[idx];
                // Slice ends at the next point, or the end of the file for
                // the last one; an unsorted successor clamps to a zero-length
                // slice.
                let end_p = if idx + 1 < n {
                    slice.points[idx + 1]
                } else {
                    1.0
                };
                let end_p = end_p.clamp(0.0, 1.0).max(start_p);
                let slice_lo = (start_p * frame_count as f64).min(max_frame);
                let slice_hi = end_p * frame_count as f64;
                // A looping window ends exactly where the next slice (or the
                // file) begins, so the repeat period is the slice length.
                let slice_hi = if self.params.mode.loops() {
                    slice_hi
                } else {
                    slice_hi.min(max_frame)
                };
                (slice_lo + frac * (slice_hi - slice_lo), slice_hi)
            }
            None if self.params.mode.loops() => (0.0, frame_count as f64),
            None => (0.0, max_frame),
        }
    }
}

impl crate::types::PatchUpdateHandler for Sampler {
    fn on_patch_update(&mut self) {
        // wav.sample_rate() is only valid after connect() resolves the WAV data,
        // so the rate ratio is computed here rather than in init.
        let wav_rate = self.params.wav.sample_rate() as f64;
        let engine_rate = self.state.sample_rate as f64;
        self.state.rate_ratio = if wav_rate > 0.0 && engine_rate > 0.0 {
            wav_rate / engine_rate
        } else {
            1.0
        };
        let fade_secs = self.params.fade / 1000.0;
        self.state.fade_frames = fade_secs * wav_rate;
        // A fade shorter than a sample still lasts one, so a timed release
        // never ends before it sounds.
        let fade_out_samples = (fade_secs * engine_rate).round() as u32;
        self.state.fade_out_samples = if self.params.fade > 0.0 {
            fade_out_samples.max(1)
        } else {
            0
        };
    }
}

message_handlers!(impl Sampler {});

#[cfg(test)]
mod tests;
