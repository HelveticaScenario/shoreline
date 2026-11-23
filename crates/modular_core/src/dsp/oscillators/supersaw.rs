use crate::bounds::Bounds;
use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;
use crate::{
    dsp::{
        core::pan::pan_pair,
        oscillators::{
            FmMode, Landing, Reset, RestartEdge, SyncRatios, apply_fm, sync_blep, sync_ratio,
            synced_increment,
        },
        utils::{
            fade::{DEFAULT_FADE_IN_SECS, FadeIn},
            phase_delta, sanitize,
            unison_phase::{UnisonPhases, phase_glide_step, voice_phase},
            unison_voices::{
                SlotOrder, centred_position, detune_octaves, spread_position, unison_gain,
            },
            wrap_phase,
        },
    },
    poly::{PORT_MAX_LANES, PolyOutput, PolySignal, PolySignalExt},
};

fn default_unison() -> usize {
    5
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
struct SupersawParams {
    #[input(
        "the note's pitch; with `sync` set, each saw restarts at its own detuned copy of it",
        type = pitch
    )]
    freq: PolySignal,
    #[input(
        "number of detuned saws stacked per note, spread across the stereo field",
        range = (1, 64),
    )]
    #[serde(default = "default_unison")]
    #[deserr(default = default_unison())]
    unison: usize,
    #[input(
        "spread between the lowest and highest saw, in semitones",
        type = control,
        default = 0.18,
        range = (0.0, ..),
        control_range = (0.0, 12.0),
    )]
    #[deserr(default)]
    detune: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp)",
        type = audio,
    )]
    #[deserr(default)]
    fm: Option<PolySignal>,
    #[input("how `fm` bends the frequency")]
    #[serde(default)]
    #[deserr(default)]
    fm_mode: FmMode,
    #[input(
        "hard sync: how many octaves above its restart pitch each saw runs; sweep it for sync tearing",
        type = control,
        default = 0.0,
        range = (0.0, ..),
        control_range = (0.0, 3.0),
    )]
    #[deserr(default)]
    #[block_input(optional)]
    sync: Option<PolySignal>,
    #[input("each rising edge restarts every saw at its starting phase", type = trig)]
    #[deserr(default)]
    #[block_input(optional)]
    restart: Option<PolySignal>,
    #[input(
        "each new saw stays silent until it first crosses zero, so its starting phase cannot click"
    )]
    #[serde(default = "crate::dsp::utils::default_true")]
    #[deserr(default = crate::dsp::utils::default_true())]
    wait_for_zero_cross: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SupersawOutputs {
    #[output("output", "the stacked saws, one stereo lane per note", type = cv, bounds = params, default)]
    sample: PolyOutput,
}

const SUPERSAW_DOCS: ModuleDocs = ModuleDocs {
    summary: "Detuned saw stack",
    description: "A stack of detuned sawtooths for huge trance leads, pads and \
                  chords. Each note is one stereo lane, its saws spread from left to \
                  right.",
    categories: &[ReferenceCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Trance chord",
            code: r#"
                chord: $supersaw('a2:m', {
                  unison: 7,
                  detune: 0.3,
                }).lpf('c6', 1)
            "#,
        },
        DocExample {
            title: "Sync sweep",
            code: r#"
                keys: $supersaw('a2', {
                  sync: $sine('0.1hz').range(0, 3),
                })
            "#,
        },
        DocExample {
            title: "Wide lead sweep",
            code: r#"
                keys: $supersaw('a3', { detune: 0.3 }).lpf(
                  $sine('0.1hz').range('c5', 'c7'),
                )
            "#,
        },
    ],
};

#[module(name = "$supersaw", stereo = stereo, docs = SUPERSAW_DOCS, has_init, patch_update, args(freq), block)]
pub struct Supersaw {
    outputs: SupersawOutputs,
    params: SupersawParams,
    state: SupersawState,
    /// Every saw's state: a `voices * PORT_MAX_LANES` matrix indexed as
    /// `[voice * PORT_MAX_LANES + lane]`, where voices count from lowest to
    /// highest pitch and lane `2n + c` is side `c` of note `n`. Each side
    /// runs its own saws from its own side's inputs. The row stride is the
    /// fixed `PORT_MAX_LANES`, never the voice count, so the element-wise
    /// state transfer keeps every surviving voice's cells at the same
    /// coordinates when the `unison` param changes; only the voice dimension
    /// grows or shrinks.
    lane_state: Box<[VoiceState]>,
}

/// One saw of the voice-mixing matrix and its sync source. Lives in
/// `lane_state` (not `state`) so it rides the size-aware per-lane transfer
/// instead of the wholesale `state` swap.
#[derive(Default, Clone, Copy)]
struct VoiceState {
    /// The saw's phase accumulator in `[0, 1)`, read shifted by the voice's
    /// phase offset.
    phase: f32,
    /// The sync source's phase accumulator: a cycle at the voice's pitch,
    /// read shifted by `sync_offset`. Unsynced it moves with the saw, so
    /// turning sync on or off moves nothing.
    sync_phase: f32,
    /// The phase offset the sync source was last read with.
    sync_offset: f32,
    restart: RestartEdge,
    /// PolyBLEP residual carried into the next sample from a reset.
    blep_carry: f32,
    /// Whether the saw reset on the last sample. The reset's carried
    /// residual band-limits the next sample, so the saw's own wrap
    /// correction must not fire there as well.
    restarted: bool,
    /// Whether this voice's zero-cross mute has released. Stays false while the
    /// voice is held silent after a fresh start, then latches true at the first
    /// output zero crossing. Carried-over voices arrive with this true.
    gate_open: bool,
    /// Previous (un-muted) saw value, used to detect the releasing zero
    /// crossing.
    prev_out: f32,
}

/// Module-level state for the Supersaw module.
struct SupersawState {
    /// Each voice's phase offset; its count is the voice count.
    phases: UnisonPhases,
    /// Reciprocal of the sample rate.
    inv_sample_rate: f32,
    /// Each voice's place in the detune spread, from -0.5 for the lowest
    /// voice to 0.5 for the highest.
    positions: [f32; PORT_MAX_LANES],
    /// Each voice's gain into each side, `[side][voice]`: 5 V over the square
    /// root of the voice count, so the stack keeps its loudness whatever the
    /// voice count, spread across the stereo slots in `SlotOrder` with
    /// equal-power panning that keeps a centred voice at unity.
    mix: [[f32; PORT_MAX_LANES]; 2],
    /// Each side's loudest swing, every saw at its peak at once: the output
    /// bounds.
    peak: [f32; 2],
    /// `mix` and `peak` as they were when the voice count last changed; the
    /// stack crossfades from them to the new ones over `mix_fade`.
    mix_from: [[f32; PORT_MAX_LANES]; 2],
    peak_from: [f32; 2],
    /// Progress of the crossfade from `mix_from` to `mix`; 1 once done.
    mix_fade: FadeIn,
    /// Per-sample step of `mix_fade`.
    mix_fade_step: f32,
}

// Hand-written `Default`: the standard library only derives `Default` for
// arrays up to length 32, so the 64-wide arrays need explicit initializers.
impl Default for SupersawState {
    fn default() -> Self {
        Self {
            phases: UnisonPhases::default(),
            inv_sample_rate: 0.0,
            positions: [0.0; PORT_MAX_LANES],
            mix: [[0.0; PORT_MAX_LANES]; 2],
            peak: [0.0; 2],
            mix_from: [[0.0; PORT_MAX_LANES]; 2],
            peak_from: [0.0; 2],
            mix_fade: FadeIn { gain: 1.0 },
            mix_fade_step: 0.0,
        }
    }
}

/// PolyBLEP correction for sawtooth wave discontinuity at phase wrap.
#[inline(always)]
fn poly_blep_saw(phase: f32, dt: f32) -> f32 {
    // Near phase = 0 (just after wrap)
    if phase < dt {
        let t = phase / dt;
        return t + t - t * t - 1.0;
    }
    // Near phase = 1 (just before wrap)
    if phase > 1.0 - dt {
        let t = (phase - 1.0) / dt;
        return t * t + t + t + 1.0;
    }
    0.0
}

impl Supersaw {
    fn init(&mut self, sample_rate: f32) {
        // Sample-rate-derived: safe in init because the rate never changes across
        // a patch transfer (a rate change rebuilds the processor, not a transfer).
        self.state.inv_sample_rate = 1.0 / sample_rate;
        self.state.mix_fade_step = FadeIn::increment(DEFAULT_FADE_IN_SECS, sample_rate);

        // One `PORT_MAX_LANES`-wide row per voice. Input lanes are
        // bounded by PORT_MAX_LANES, so a full-width row covers any input
        // width, and the fixed stride keeps each voice's cells at stable flat
        // indices across a voice-count change — the element-wise transfer then
        // carries surviving voices intact while added voices keep the fresh
        // state below.
        let voices = self.params.unison.clamp(1, PORT_MAX_LANES);
        self.lane_state = vec![VoiceState::default(); voices * PORT_MAX_LANES].into_boxed_slice();
        // Arm each fresh voice's zero-cross mute. prev_out is the naive saw
        // at the voice's starting phase so the mute holds until the output
        // genuinely crosses zero (it would release immediately if left at 0).
        // For carried-over voices these values are overwritten by
        // `transfer_state_from`, keeping an already-sounding voice un-muted.
        // The sync source starts read at the same offset as the saw, so its
        // first sample sees no glide.
        let gate_open = !self.params.wait_for_zero_cross;
        for (voice, row) in self.lane_state.chunks_mut(PORT_MAX_LANES).enumerate() {
            let offset = voice_phase(voice, voices);
            for cell in row {
                cell.gate_open = gate_open;
                cell.prev_out = 2.0 * offset - 1.0;
                cell.sync_offset = offset;
            }
        }
    }

    /// Recompute param- and sample-rate-derived constants. Invoked from
    /// `on_patch_update`, which runs after `transfer_state_from` swaps `state`,
    /// so these reflect the current params — not a transferred predecessor's.
    fn configure(&mut self) {
        let voices = self.params.unison.clamp(1, PORT_MAX_LANES);
        let state = &mut self.state;
        let fresh = state.phases.count() == 0;
        let changed = state.phases.set_count(voices);
        for (voice, position) in state.positions.iter_mut().enumerate().take(voices) {
            *position = spread_position(voice, voices);
        }

        let gain = 5.0 * unison_gain(voices);
        let order = SlotOrder::new(voices);
        let mut mix = [[0.0f32; PORT_MAX_LANES]; 2];
        for slot in 0..voices {
            let voice = order.detune(slot);
            let [left, right] = pan_pair([gain, gain], centred_position(slot, voices));
            mix[0][voice] = left;
            mix[1][voice] = right;
        }
        let peak = mix.map(|side| side[..voices].iter().sum::<f32>());

        // A new voice count changes every voice's gain and place; the voices
        // both counts hold crossfade from where the mix stands to their new
        // gains, so the level and image glide. Voices the new count drops
        // stop at once.
        if changed && !fresh {
            state.mix_from = state.current_mix();
            state.peak_from = state.current_peak();
            state.mix_fade = FadeIn::default();
        }
        state.mix = mix;
        state.peak = peak;
    }

    /// Lane `lane`'s bounds: its side's peak either way.
    fn bounds(&self, lane: usize) -> Bounds {
        let peak = self.state.current_peak()[lane % 2];
        Bounds::new(-peak, peak)
    }

    fn update(&mut self, sample_rate: f32) {
        let voices = self.state.phases.count();
        let lanes = self.lane_count();
        let p = &self.params;
        self.state.mix_fade.advance(self.state.mix_fade_step);
        let mix = self.state.current_mix();
        let offsets = self.state.phases.advance(phase_glide_step(sample_rate));
        for lane in 0..lanes {
            let pitch = p.freq.get_value(lane);
            let fm = p.fm.value_or(lane, 0.0);
            let base_freq = apply_fm(pitch, fm, p.fm_mode);
            let detune = p.detune.value_or(lane, SupersawParams::DETUNE_DEFAULT);
            let ratio = p.sync.as_ref().map(|sync| sync_ratio(sync.get_value(lane)));
            let restart = p.restart.as_ref().map(|restart| restart.get_value(lane));
            let mut accum = 0.0f32;
            for (voice, &gain) in mix[lane % 2].iter().enumerate().take(voices) {
                let dt = voice_increment(
                    self.state.positions[voice],
                    base_freq,
                    detune,
                    self.state.inv_sample_rate,
                );
                let cell = &mut self.lane_state[voice * PORT_MAX_LANES + lane];
                accum += cell.step(dt, offsets[voice], restart, ratio) * gain;
            }
            self.outputs.sample.set(lane, accum);
        }
        if self.outputs.sample.stores_bounds() {
            for lane in 0..lanes {
                let bounds = self.bounds(lane);
                self.outputs.sample.set_bounds(lane, bounds);
            }
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut SupersawBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        // The per-sample path runs the crossfade after a voice-count change.
        if self.state.mix_fade.gain < 1.0 {
            return None;
        }
        let voices = self.state.phases.count();
        let lanes = self.lane_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let SupersawParamsBlockInputs {
            freq,
            detune,
            fm,
            sync,
            restart,
        } = p.block_inputs(range.clone(), lanes, &mut scratch)?;

        // Each voice's phase increment, `[voice * lanes + lane]`. A held
        // pitch, FM and detune fill them once for the whole range.
        let mut increments = [0.0f32; PORT_MAX_LANES * PORT_MAX_LANES];
        let mut ratios = SyncRatios::new(sync, range.start);
        let pitch_held = freq.is_constant() && fm.is_constant() && detune.is_constant();
        let (positions, inv_sample_rate) = (&self.state.positions, self.state.inv_sample_rate);
        let fill_increments = |increments: &mut [f32], i: usize| {
            let (freq, fm, detune) = (freq.row(i), fm.row(i), detune.row(i));
            for lane in 0..lanes {
                let base_freq = apply_fm(freq[lane], fm[lane], p.fm_mode);
                for voice in 0..voices {
                    increments[voice * lanes + lane] =
                        voice_increment(positions[voice], base_freq, detune[lane], inv_sample_rate);
                }
            }
        };
        if pitch_held {
            fill_increments(&mut increments, range.start);
        }
        let peak = self.state.peak;
        outputs.sample.fill_bounds_with(range.clone(), |lane| {
            Some(Bounds::new(-peak[lane % 2], peak[lane % 2]))
        });

        for i in range {
            if !pitch_held {
                fill_increments(&mut increments, i);
            }
            let ratios = ratios.at(i);
            let offsets = self.state.phases.advance(phase_glide_step(sample_rate));
            let restart = restart.as_ref().map(|restart| restart.row(i));
            let out = outputs.sample.slot_mut(i);
            for (lane, out) in out.iter_mut().enumerate().take(lanes) {
                let mix = &self.state.mix[lane % 2];
                let mut accum = 0.0f32;
                for voice in 0..voices {
                    let dt = increments[voice * lanes + lane];
                    let cell = &mut self.lane_state[voice * PORT_MAX_LANES + lane];
                    let restart = restart.map(|restart| restart[lane]);
                    accum += cell.step(dt, offsets[voice], restart, ratios[lane]) * mix[voice];
                }
                *out = sanitize(accum);
            }
        }
        Some(())
    }
}

impl SupersawState {
    /// Each voice's gain into each side this sample, partway through the
    /// crossfade after a voice-count change.
    fn current_mix(&self) -> [[f32; PORT_MAX_LANES]; 2] {
        let t = self.mix_fade.gain;
        if t >= 1.0 {
            return self.mix;
        }
        let mut mix = self.mix_from;
        for (side, to) in mix.iter_mut().zip(&self.mix) {
            for (gain, &to) in side.iter_mut().zip(to) {
                *gain += (to - *gain) * t;
            }
        }
        mix
    }

    /// Each side's loudest swing this sample: during the crossfade, the
    /// larger of the two mixes' peaks.
    fn current_peak(&self) -> [f32; 2] {
        if self.mix_fade.gain >= 1.0 {
            self.peak
        } else {
            [0, 1].map(|side| self.peak[side].max(self.peak_from[side]))
        }
    }
}

/// Phase increment of the voice at `position` in the detune spread for a
/// lane at `base_freq` Hz with `detune` semitones of spread.
#[inline(always)]
fn voice_increment(position: f32, base_freq: f32, detune: f32, inv_sample_rate: f32) -> f32 {
    let freq = base_freq * (2.0f32).powf(detune_octaves(position, detune));
    freq * inv_sample_rate
}

impl VoiceState {
    /// Advance by `dt`, the voice's increment at the note's pitch, and return
    /// its contribution, the saw read `offset` cycles ahead. With a
    /// `sync_ratio`, the saw runs that many times faster and restarts each
    /// time its sync source completes a cycle at `dt`. Each rising edge on
    /// `restart` restarts the saw and its source at their starting phase,
    /// winning over a sync reset that lands with it.
    #[inline(always)]
    fn step(&mut self, dt: f32, offset: f32, restart: Option<f32>, sync_ratio: Option<f32>) -> f32 {
        let restarted = restart.and_then(|v| self.restart.process(v));
        let (saw_dt, wrapped) = match sync_ratio {
            Some(ratio) => (
                synced_increment(dt, ratio),
                self.advance_sync_source(dt, offset),
            ),
            None => (dt, None),
        };
        let reset = Reset::first(restarted, wrapped);
        let mut phase = wrap_phase(self.phase + saw_dt);
        let read = wrap_phase(phase + offset);

        // Naive saw + its own PolyBLEP wrap correction, then any residual
        // carried from a reset on the previous sample. The reset (below)
        // lands in the upcoming interval, so this operates on the real,
        // pre-reset phase. A reset band-limits the jump it makes, which
        // replaces the wrap ahead on the sample it lands and the wrap just
        // behind on the sample after it.
        let mut saw = 2.0 * read - 1.0;
        let replaced = if read < saw_dt.abs() {
            self.restarted
        } else {
            reset.is_some()
        };
        if !replaced {
            saw -= poly_blep_saw(read, saw_dt.abs());
        }
        saw += self.blep_carry;
        self.blep_carry = 0.0;
        self.restarted = reset.is_some();

        // A reset restarts the saw, band-limited with a PolyBLEP at the
        // subsample point `frac` into the next interval. A sync reset puts a
        // rising saw at the bottom of its ramp and a falling one at the top;
        // a restart puts the saw and its source back at the voice's starting
        // phase. Either way the phase sits `frac` of a step before the
        // restart, so the next sample lands on the ramp the restart began.
        if let Some(reset) = reset {
            let frac = reset.frac();
            // The saw as it reaches the reset; one that would wrap first
            // counts as reaching the end of its ramp.
            let before = 2.0 * (read + frac * saw_dt).clamp(0.0, 1.0) - 1.0;
            let after = match reset {
                Reset::Sync(_) => {
                    phase = wrap_phase(-offset - frac * saw_dt);
                    if saw_dt < 0.0 { 1.0 } else { -1.0 }
                }
                Reset::Restart(_) => {
                    phase = wrap_phase(-frac * saw_dt);
                    self.sync_phase = wrap_phase(-frac * dt);
                    self.sync_offset = offset;
                    let start = wrap_phase(offset);
                    if saw_dt < 0.0 && start == 0.0 {
                        1.0
                    } else {
                        2.0 * start - 1.0
                    }
                }
            };
            let (now, carry) = sync_blep(after - before, frac);
            saw += now;
            self.blep_carry = carry;
            // A restarted saw that starts just short of its wrap crosses it
            // before the next sample, which leaves it to the carry.
            if let Reset::Restart(_) = reset {
                let start = wrap_phase(offset);
                let (ahead, jump) = if saw_dt < 0.0 {
                    (start, 2.0)
                } else {
                    ((1.0 - start).rem_euclid(1.0), -2.0)
                };
                let at = frac + ahead / saw_dt.abs();
                if ahead > 0.0 && at < 1.0 {
                    let (now, carry) = sync_blep(jump, at);
                    saw += now;
                    self.blep_carry += carry;
                }
            }
        }

        self.phase = phase;
        if sync_ratio.is_none() {
            self.sync_phase = phase;
            self.sync_offset = offset;
        }

        // Zero-cross mute: hold this voice silent until its saw rises
        // through zero, then latch open. A saw also crosses zero on its
        // steep wrap (positive→negative) — releasing there would click —
        // so only the rising (negative→positive) crossing on the ramp
        // counts. Releasing on the ramp means the first contributed
        // sample is ~0.
        let contribution = if self.gate_open {
            saw
        } else if self.prev_out <= 0.0 && saw >= 0.0 {
            self.gate_open = true;
            saw
        } else {
            0.0
        };
        self.prev_out = saw;
        contribution
    }

    /// Advance the sync source by `dt`, read `offset` ahead; when it
    /// completes a cycle in the step after this one, where the saw's resets
    /// land, returns how far into that step it does. The read moves by `dt`
    /// plus however far the offset glided, so a gliding offset neither skips
    /// nor repeats a cycle.
    #[inline(always)]
    fn advance_sync_source(&mut self, dt: f32, offset: f32) -> Option<f32> {
        let before = wrap_phase(self.sync_phase + self.sync_offset);
        let glided = phase_delta(self.sync_offset, offset);
        self.sync_phase = wrap_phase(self.sync_phase + dt);
        self.sync_offset = offset;
        Landing::Next.wrap(before, dt + glided)
    }
}

impl crate::types::PatchUpdateHandler for Supersaw {
    fn on_patch_update(&mut self) {
        self.configure();
    }
}

message_handlers!(impl Supersaw {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Connect, LaneExpansion, OutputStruct, Signal};

    fn params(freq: PolySignal, unison: usize, detune: f32) -> SupersawParams {
        SupersawParams {
            freq,
            unison,
            detune: Some(PolySignal::mono(Signal::Volts(detune))),
            fm: None,
            fm_mode: FmMode::default(),
            sync: None,
            restart: None,
            wait_for_zero_cross: false,
        }
    }

    /// A Supersaw built as a patch builds it: inputs laid out side by side,
    /// one lane per side of each note, then `init` and `on_patch_update`.
    fn make_supersaw(mut params: SupersawParams) -> Supersaw {
        use crate::types::PatchUpdateHandler;
        let lanes = params.freq.lanes() * 2;
        params.expand_lanes(&LaneExpansion {
            channels: 2,
            producer: &|_, _| 1,
        });
        let mut outputs = SupersawOutputs::default();
        outputs.set_all_lanes(lanes);
        let mut s = Supersaw {
            params,
            outputs,
            _lane_count: lanes,
            _channels: 2,
            _block_index: Default::default(),
            _connected: Default::default(),
            state: SupersawState::default(),
            lane_state: Box::default(),
        };
        s.init(48000.0);
        s.on_patch_update();
        s
    }

    fn mono(v: f32) -> PolySignal {
        PolySignal::mono(Signal::Volts(v))
    }

    #[test]
    fn each_note_is_one_stereo_lane() {
        let s = make_supersaw(params(
            PolySignal::poly(&[Signal::Volts(0.0), Signal::Volts(1.0), Signal::Volts(2.0)]),
            7,
            0.18,
        ));
        assert_eq!(s.lane_count(), 6);
    }

    #[test]
    fn voices_clamp_to_the_port() {
        assert_eq!(
            make_supersaw(params(mono(0.0), 100, 0.18))
                .state
                .phases
                .count(),
            PORT_MAX_LANES
        );
        assert_eq!(
            make_supersaw(params(mono(0.0), 0, 0.18))
                .state
                .phases
                .count(),
            1
        );
    }

    /// Carries `old` into `new` as a patch update does: the whole `state`
    /// swaps, the `lane_state` cells both hold carry over, then
    /// `on_patch_update` runs.
    fn carry(old: &mut Supersaw, new: &mut Supersaw) {
        use crate::types::PatchUpdateHandler;
        std::mem::swap(&mut new.state, &mut old.state);
        let cells = old.lane_state.len().min(new.lane_state.len());
        new.lane_state[..cells].copy_from_slice(&old.lane_state[..cells]);
        new.on_patch_update();
    }

    fn synced(octaves: f32, base: SupersawParams) -> SupersawParams {
        SupersawParams {
            sync: Some(mono(octaves)),
            ..base
        }
    }

    #[test]
    fn gain_follows_the_voice_count() {
        // Equal-power panning: a voice's two sides square-sum to twice its
        // gain squared.
        let gain = |s: &Supersaw, v: usize| {
            ((s.state.mix[0][v].powi(2) + s.state.mix[1][v].powi(2)) / 2.0).sqrt()
        };
        let four = make_supersaw(params(mono(0.0), 4, 0.18));
        for v in 0..4 {
            assert!((gain(&four, v) - 2.5).abs() < 1e-6, "voice {v}");
        }
        assert_eq!(
            make_supersaw(params(mono(0.0), 1, 0.18)).state.mix[0][0],
            5.0
        );
    }

    #[test]
    fn output_stays_bounded() {
        let mut s = make_supersaw(params(mono(0.0), 5, 0.18));
        for _ in 0..1000 {
            s.update(48000.0);
            for lane in 0..2 {
                let v = s.outputs.sample.get(lane);
                assert!(v.is_finite() && v.abs() <= 10.0, "lane {lane}: {v}");
            }
        }
    }

    #[test]
    fn a_single_voice_sits_in_the_centre() {
        let mut s = make_supersaw(params(mono(0.0), 1, 0.0));
        s.lane_state[0].phase = 0.75;
        s.lane_state[1].phase = 0.75;
        s.update(48000.0);
        let (left, right) = (s.outputs.sample.get(0), s.outputs.sample.get(1));
        assert_eq!(left, right);
        assert!(left > 0.0);
    }

    #[test]
    fn voices_spread_from_left_to_right() {
        // Two voices: the first hard left, the second hard right.
        let mut s = make_supersaw(params(mono(0.0), 2, 0.0));
        // Each saw reads its phase shifted by its voice's offset.
        for lane in 0..2 {
            s.lane_state[lane].phase = 0.75 - voice_phase(0, 2);
            s.lane_state[PORT_MAX_LANES + lane].phase = 0.25 - voice_phase(1, 2);
        }
        s.update(48000.0);
        let (left, right) = (s.outputs.sample.get(0), s.outputs.sample.get(1));
        assert!(left > 0.0 && right < 0.0, "left {left}, right {right}");
    }

    #[test]
    fn each_side_holds_a_spread_of_pitches() {
        // Five voices, lowest pitch first, take the slots left to right as
        // 1 4 2 0 3: each side holds one voice below the note and one above,
        // and the in-tune voice sits in the centre.
        let s = make_supersaw(params(mono(0.0), 5, 0.18));
        let [left, right] = s.state.mix;
        for v in [1, 4] {
            assert!(left[v] > right[v], "voice {v} leans left");
        }
        assert_eq!(left[2], right[2], "voice 2 sits in the centre");
        for v in [0, 3] {
            assert!(left[v] < right[v], "voice {v} leans right");
        }
    }

    #[test]
    fn detune_spreads_the_voices() {
        let run = |detune: f32| {
            let mut s = make_supersaw(params(mono(0.0), 3, detune));
            for cell in s.lane_state.iter_mut() {
                cell.phase = 0.25;
            }
            for _ in 0..100 {
                s.update(48000.0);
            }
            (
                s.lane_state[0].phase,
                s.lane_state[2 * PORT_MAX_LANES].phase,
            )
        };
        let (a, b) = run(0.0);
        assert_eq!(a, b);
        let (a, b) = run(2.0);
        assert_ne!(a, b);
    }

    #[test]
    fn a_stereo_pitch_detunes_one_side_against_the_other() {
        let mut s = make_supersaw(params(mono(0.0), 1, 0.0));
        s.params.freq = PolySignal::poly(&[Signal::Volts(0.0), Signal::Volts(1.0)]);
        for _ in 0..100 {
            s.update(48000.0);
        }
        assert_ne!(s.lane_state[0].phase, s.lane_state[1].phase);
    }

    #[test]
    fn a_unison_change_neither_steps_the_level_nor_jumps_a_saw() {
        // Around 1 Hz no saw wraps in the window, so every sample-to-sample
        // move is the ramps, the phase glide and the level crossfade. A
        // stepped gain or a jumped phase would move the output by volts.
        let sample_rate = 48000.0;
        let mut old = make_supersaw(params(mono(-8.0), 5, 0.18));
        for _ in 0..100 {
            old.update(sample_rate);
        }
        let mut prev = [old.outputs.sample.get(0), old.outputs.sample.get(1)];
        let mut new = make_supersaw(SupersawParams {
            wait_for_zero_cross: true,
            ..params(mono(-8.0), 7, 0.18)
        });
        carry(&mut old, &mut new);
        for i in 0..1000 {
            new.update(sample_rate);
            for (lane, prev) in prev.iter_mut().enumerate() {
                let v = new.outputs.sample.get(lane);
                assert!(
                    (v - *prev).abs() < 0.05,
                    "sample {i} lane {lane}: {prev} -> {v}"
                );
                *prev = v;
            }
        }
        assert_eq!(new.state.mix_fade.gain, 1.0, "the crossfade completes");
    }

    #[test]
    fn sync_sets_how_much_faster_the_saw_runs() {
        let mut s = make_supersaw(synced(1.5, params(mono(0.0), 1, 0.0)));
        s.update(48000.0);
        let (saw, source) = (s.lane_state[0].phase, s.lane_state[0].sync_phase);
        s.update(48000.0);
        let ratio = (s.lane_state[0].phase - saw) / (s.lane_state[0].sync_phase - source);
        assert!((ratio - 1.5f32.exp2()).abs() < 1e-3, "{ratio}");
    }

    #[test]
    fn each_saw_restarts_on_its_own_detuned_source() {
        // Two semitones of detune: the top voice's source runs faster than
        // the bottom one's, and each saw restarts as its own source wraps.
        let mut s = make_supersaw(synced(1.0, params(mono(0.0), 3, 2.0)));
        let source = |s: &Supersaw, voice: usize| {
            let cell = &s.lane_state[voice * PORT_MAX_LANES];
            wrap_phase(cell.sync_phase + cell.sync_offset)
        };
        let mut restarts = [0usize; 3];
        for _ in 0..48000 {
            let before: Vec<f32> = (0..3).map(|v| source(&s, v)).collect();
            s.update(48000.0);
            for (voice, restarts) in restarts.iter_mut().enumerate() {
                let cell = &s.lane_state[voice * PORT_MAX_LANES];
                if source(&s, voice) < before[voice] {
                    *restarts += 1;
                    // The source looks a step ahead, so its saw restarted on
                    // the step before and stands at most one step of the
                    // saw past the start of its ramp.
                    let read = wrap_phase(cell.phase + voice_phase(voice, 3));
                    let past = phase_delta(0.0, read);
                    assert!(
                        (0.0..=2.0 * 600.0 / 48000.0).contains(&past),
                        "voice {voice} restarts at {read}"
                    );
                }
            }
        }
        // About 247 and 277 Hz, a semitone either side of middle C.
        assert!((245..=249).contains(&restarts[0]), "{restarts:?}");
        assert!((275..=279).contains(&restarts[2]), "{restarts:?}");
    }

    #[test]
    fn a_fresh_synced_stack_starts_on_its_phases() {
        // Every voice's source starts read where its saw is, so no saw
        // restarts until its source completes a cycle.
        let mut s = make_supersaw(synced(1.0, params(mono(0.0), 7, 0.18)));
        s.update(48000.0);
        for voice in 0..7 {
            let cell = &s.lane_state[voice * PORT_MAX_LANES];
            assert!(!cell.restarted, "voice {voice} restarted at once");
        }
    }

    #[test]
    fn turning_sync_on_moves_nothing() {
        // With `sync` at 0 each saw restarts exactly when it would
        // wrap anyway, so switching it on mid-note leaves the output where
        // it was.
        let render = |switch_at: Option<usize>| {
            let mut s = make_supersaw(params(mono(-6.0), 3, 0.18));
            (0..3000)
                .map(|i| {
                    if Some(i) == switch_at {
                        s.params.sync = Some(mono(0.0));
                    }
                    s.update(48000.0);
                    s.outputs.sample.get(0)
                })
                .collect::<Vec<f32>>()
        };
        for (i, (free, switched)) in render(None).iter().zip(render(Some(1000))).enumerate() {
            assert!(
                (free - switched).abs() < 0.01,
                "sample {i}: {free} vs {switched}"
            );
        }
    }

    #[test]
    fn negative_sync_runs_the_saw_at_its_source() {
        let render = |octaves: f32| {
            let mut s = make_supersaw(synced(octaves, params(mono(0.0), 1, 0.0)));
            (0..500)
                .map(|_| {
                    s.update(48000.0);
                    s.outputs.sample.get(0)
                })
                .collect::<Vec<f32>>()
        };
        assert_eq!(render(-2.0), render(0.0));
    }

    #[test]
    fn a_falling_synced_saw_restarts_at_the_top() {
        // Through-zero FM runs the note backwards; each restart puts the saw
        // back at the top of its ramp, where a falling saw starts.
        let mut s = make_supersaw(SupersawParams {
            fm: Some(mono(-2.0)),
            fm_mode: FmMode::ThroughZero,
            ..synced(1.0, params(mono(0.0), 1, 0.0))
        });
        let mut restarts = 0;
        for _ in 0..2000 {
            let before = s.lane_state[0].sync_phase;
            s.update(48000.0);
            if s.lane_state[0].sync_phase > before {
                restarts += 1;
                s.update(48000.0);
                let read = s.lane_state[0].phase;
                assert!(read > 0.97, "the saw starts near the top, at {read}");
            }
        }
        assert!(restarts > 5, "{restarts}");
    }

    #[test]
    fn synced_output_stays_within_its_bounds_at_any_speed() {
        // High notes and far sync run the saws up to Nyquist, where the
        // band-limiting corrections are largest.
        for (freq, voices) in [(3.0, 1), (4.0, 5), (1.0, 7)] {
            let mut s = make_supersaw(synced(0.0, params(mono(freq), voices, 0.3)));
            let peak = s.state.peak;
            for i in 0..8000 {
                s.params.sync = Some(mono(i as f32 / 1000.0));
                s.update(48000.0);
                for (lane, peak) in peak.iter().enumerate() {
                    let v = s.outputs.sample.get(lane);
                    assert!(
                        v.is_finite() && v.abs() <= peak + 1e-4,
                        "{freq} V, {voices} voices, sample {i}: {v} past {peak}"
                    );
                }
            }
        }
    }

    #[test]
    fn wait_for_zero_cross_mutes_fresh_voice_until_crossing() {
        // A fresh voice that does not start on a zero crossing is held silent
        // until its saw crosses zero, then releases on a near-zero sample.
        let mut s = make_supersaw(SupersawParams {
            wait_for_zero_cross: true,
            ..params(mono(0.0), 1, 0.0)
        });
        s.lane_state[0].phase = 0.25;
        s.lane_state[0].prev_out = -0.5;
        s.lane_state[0].gate_open = false;

        let out: Vec<f32> = (0..200)
            .map(|_| {
                s.update(48_000.0);
                s.outputs.sample.get(0)
            })
            .collect();

        assert_eq!(out[0], 0.0, "first sample should be muted");
        let release = out.iter().position(|&v| v != 0.0).expect("must release");
        assert!(
            release > 5,
            "should hold silence while the saw climbs to zero, released at {release}"
        );
        assert!(
            out[release].abs() < 0.5,
            "release sample {} should be near zero (click-free)",
            out[release]
        );
    }

    /// Sync at 0 restarts each cycle exactly where it wraps anyway, so from
    /// the first sample, across restarts and with sync switched on partway,
    /// the stack is the free one.
    #[test]
    fn sync_at_zero_is_the_free_stack() {
        let render = |sync_from: Option<usize>| {
            let mut osc = make_supersaw(params(mono(2.0), 3, 0.18));
            (0..4000)
                .map(|i| {
                    osc.params.restart = Some(mono(if i % 1000 < 2 { 5.0 } else { 0.0 }));
                    if sync_from == Some(i) {
                        osc.params.sync = Some(mono(0.0));
                    }
                    osc.update(48000.0);
                    osc.outputs.sample.get(0)
                })
                .collect::<Vec<f32>>()
        };
        let free = render(None);
        for sync_from in [0, 1537] {
            for (i, (synced, free)) in render(Some(sync_from)).iter().zip(&free).enumerate() {
                assert!(
                    (synced - free).abs() < 1e-3,
                    "synced from {sync_from}, sample {i}: {synced} vs {free}"
                );
            }
        }
    }

    #[test]
    fn restart_starts_every_saw_where_the_trigger_crosses() {
        let mut s = make_supersaw(SupersawParams {
            restart: Some(mono(0.0)),
            ..params(mono(0.0), 5, 0.3)
        });
        for _ in 0..300 {
            s.update(48000.0);
        }
        s.params.restart = Some(mono(5.0));
        s.update(48000.0);
        s.update(48000.0);
        // A trigger from 0 to 5 V crosses its threshold a fifth of the way
        // through the sample, so the sample after the restart lands stands
        // four fifths of each saw's step into its new cycle, its source with
        // it.
        let note = crate::dsp::utils::voct_to_hz(0.0);
        for voice in 0..5 {
            let cell = &s.lane_state[voice * PORT_MAX_LANES];
            let step = voice_increment(s.state.positions[voice], note, 0.3, 1.0 / 48000.0);
            assert!(
                (cell.phase - 0.8 * step).abs() < 1e-6,
                "voice {voice}: {}",
                cell.phase
            );
            assert_eq!(cell.sync_phase, cell.phase, "voice {voice}");
        }
    }

    #[test]
    fn a_restarted_saw_band_limits_a_wrap_it_crosses_at_once() {
        // Restarted a fifth of the way into a step, half a step short of its
        // wrap, the saw crosses it at 0.7 of the step: the samples either side
        // share its band-limiting, and neither overshoots.
        let mut cell = VoiceState {
            phase: 0.3,
            gate_open: true,
            ..Default::default()
        };
        let first = cell.step(0.01, 0.995, Some(5.0), None);
        let second = cell.step(0.01, 0.995, Some(5.0), None);
        assert!((first + 0.0397).abs() < 1e-4, "{first}");
        assert!((second + 0.5315).abs() < 1e-4, "{second}");
    }
}
