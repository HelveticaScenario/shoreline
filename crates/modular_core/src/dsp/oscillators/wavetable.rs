//! `$wavetable` — band-limited wavetable oscillator with mipmap-based
//! anti-aliasing.
//!
//! The `wav` parameter is a reference to a loaded WAV file. On the main
//! thread, `prepare_resources` pre-computes an FFT-based mipmap pyramid
//! stored in `params.prepared` (a [`PreparedWavetable`]). The audio-thread
//! `update()` loop reads from that table using `params.pitch` (V/Oct),
//! `params.position` (frame index in 0–5V normalized to 0–1), and an
//! optional `params.phase` [`PhaseWarp`] that warps the raw phase before
//! sampling.
//!
//! If the WAV isn't loaded or the prepared table is empty, the oscillator
//! outputs silence (no panic, no alloc).

use deserr::Deserr;
use schemars::JsonSchema;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::ops::Range;
use std::sync::Arc;

use crate::module_docs::{DocExample, ModuleDocs};
use crate::reference_category::ReferenceCategory;
use crate::{
    dsp::{
        oscillators::{
            FmMode, Landing, Resets, SyncRatios, apply_fm, sync_corner, sync_ratio,
            wavetable_prep::{MipmapRead, PreparedWavetable},
        },
        phase_warp::{PhaseWarp, PhaseWarpGlides, PhaseWarpState, ResolvedWarp},
        utils::sanitize,
    },
    poly::{PORT_MAX_LANES, PolyOutput, PolySignal, PolySignalExt},
    types::{Wav, WavData},
};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
pub(crate) struct WavetableOscParams {
    #[input("oscillator pitch; with `sync` set, the pitch it restarts at", type = pitch)]
    pub(crate) pitch: PolySignal,
    #[input("WAV file holding the wavetable frames")]
    pub(crate) wav: Wav,
    #[input(
        "which frame plays: 0 the first, 5 the last, blending between neighbours",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    pub(crate) position: Option<PolySignal>,
    #[input(
        "frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp)",
        type = audio,
    )]
    #[deserr(default)]
    pub(crate) fm: Option<PolySignal>,
    #[input("how `fm` bends the frequency")]
    #[serde(default)]
    #[deserr(default)]
    pub(crate) fm_mode: FmMode,
    #[input(
        "hard sync: how many octaves above its restart pitch the table runs; sweep it for sync tearing",
        type = control,
        default = 0.0,
        range = (0.0, ..),
        control_range = (0.0, 3.0),
    )]
    #[deserr(default)]
    #[block_input(optional)]
    pub(crate) sync: Option<PolySignal>,
    #[input("each rising edge restarts the cycle", type = trig)]
    #[deserr(default)]
    #[block_input(optional)]
    pub(crate) restart: Option<PolySignal>,
    #[input("reshapes the cycle before the table is read, built with `$phase.*`")]
    #[deserr(default)]
    pub(crate) phase: Option<PhaseWarp>,
    #[input(
        "shifts where in the cycle the table is read, in cycles; wraps around",
        type = control,
        default = 0.0,
        range = (0.0, 1.0),
    )]
    #[deserr(default)]
    pub(crate) phase_offset: Option<PolySignal>,
    /// Pre-computed mipmap pyramid, populated by `prepare_resources` on the
    /// main thread. Skipped from serialization and deserialization.
    #[serde(skip)]
    #[deserr(skip)]
    #[schemars(skip)]
    pub(crate) prepared: Option<PreparedWavetable>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct WavetableOscOutputs {
    #[output("output", "the wavetable oscillator", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

/// Per-lane oscillator state.
#[derive(Clone, Copy, SeedSide)]
struct LaneState {
    /// Phase accumulator in `[0, 1)`.
    phase: f64,
    resets: Resets,
    /// PolyBLEP residual carried into the next sample from a reset.
    blep_carry: f32,
    /// The pitch, FM and sync ratio inputs `freq` and `mip` were derived
    /// from. Compared exactly, so the cached values are always the ones a
    /// fresh computation would give; `NaN` never matches, forcing the first
    /// computation.
    last_pitch: f32,
    last_fm: f32,
    last_ratio: f32,
    /// The note's frequency in Hz.
    freq: f32,
    /// Mipmap levels read at the table's playback frequency, `freq` times
    /// the sync ratio.
    mip: MipmapRead,
    /// State of the `phase` warp's feedback stages.
    warp: PhaseWarpState,
    /// Glides for the `phase` warp's signals.
    warp_glides: PhaseWarpGlides,
}

impl Default for LaneState {
    fn default() -> Self {
        Self {
            phase: 0.0,
            resets: Resets::default(),
            blep_carry: 0.0,
            last_pitch: f32::NAN,
            last_fm: f32::NAN,
            last_ratio: f32::NAN,
            freq: 0.0,
            mip: MipmapRead::default(),
            warp: PhaseWarpState::default(),
            warp_glides: PhaseWarpGlides::default(),
        }
    }
}

impl LaneState {
    /// Track `pitch`, `fm` and the sync `ratio`, recomputing the frequency
    /// and mipmap levels only when they change.
    #[inline(always)]
    fn tune(
        &mut self,
        prepared: &PreparedWavetable,
        pitch: f32,
        fm: f32,
        fm_mode: FmMode,
        ratio: Option<f32>,
    ) {
        let ratio = ratio.unwrap_or(1.0);
        if pitch != self.last_pitch || fm != self.last_fm || ratio != self.last_ratio {
            self.freq = apply_fm(pitch, fm, fm_mode);
            self.mip = prepared.mipmap_for_freq(self.freq * ratio);
            self.last_pitch = pitch;
            self.last_fm = fm;
            self.last_ratio = ratio;
        }
    }

    /// One output sample (±5 V) at `position` (0–5 V across the frames), then
    /// advance the phase. `warp` reshapes the read phase with its stage values
    /// already resolved for this lane; `restart` is this sample's restart
    /// input and `sync_ratio` its sync, if connected.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        prepared: &PreparedWavetable,
        sample_rate: f32,
        inv_sr: f64,
        position: f32,
        offset: f32,
        warp: Option<(&PhaseWarp, &ResolvedWarp)>,
        restart: Option<f32>,
        sync_ratio: Option<f32>,
    ) -> f32 {
        let frame_scale = if prepared.frame_count > 1 {
            (prepared.frame_count - 1) as f32
        } else {
            0.0
        };
        // Frame index: 0–5V → 0..=frame_count-1.
        let frame_f = (position.clamp(0.0, 5.0) / 5.0) * frame_scale;

        // The phase offset shifts the read position without altering the
        // accumulator, so it never drifts. The current phase is read before
        // it advances, so the first sample reads the starting phase.
        let read_offset = offset.rem_euclid(1.0);
        let raw_phase = (self.phase as f32 + read_offset).rem_euclid(1.0);
        let note_increment = self.freq as f64 * inv_sr;
        let (synced, reset) = self.resets.step(
            self.phase as f32,
            note_increment as f32,
            restart,
            sync_ratio,
            Landing::Now,
        );
        let increment = if sync_ratio.is_some() {
            synced as f64
        } else {
            note_increment
        };
        let freq_norm = if sync_ratio.is_some() {
            synced
        } else {
            self.freq / sample_rate
        };
        // The warp's state as it stood before this sample, for a reset's
        // reads below.
        let warp_before = self.warp;
        let warped_phase = match warp {
            Some((warp, resolved)) => warp.evaluate(raw_phase, resolved, freq_norm, &mut self.warp),
            None => raw_phase,
        };
        let before = prepared.read(self.mip, frame_f, warped_phase);

        // Naive read plus any residual carried from a reset on the previous
        // sample.
        let mut sample = before + self.blep_carry;
        self.blep_carry = 0.0;

        // Advance phase, wrap to [0, 1). rem_euclid in f64.
        let mut next = self.phase + increment;
        // Fast wrap for the common case; rem_euclid handles the rest.
        if next >= 1.0 {
            next -= 1.0;
            if next >= 1.0 {
                next = next.rem_euclid(1.0);
            }
        } else if next < 0.0 {
            next = next.rem_euclid(1.0);
        }
        if !next.is_finite() {
            next = 0.0;
        }

        // A reset restarts the table's cycle `frac` into this step, from
        // where the old cycle reaches it, with a PolyBLEP for the jump and a
        // PolyBLAMP for the change of slope at that point.
        if let Some(reset) = reset {
            let read_at = |phase: f32| {
                let warped = match warp {
                    Some((warp, resolved)) => {
                        let mut scratch = warp_before;
                        warp.evaluate(phase, resolved, freq_norm, &mut scratch)
                    }
                    None => phase,
                };
                prepared.read(self.mip, frame_f, warped)
            };
            // The slope in output units per sample, from a difference across
            // a small part of the cycle on the side the wave travels, so it
            // never spans a warp's jump at the cycle's start. A band-limited
            // wave slopes at most π a sample, so a steeper difference is a
            // jump inside the span, held to that.
            let span = SLOPE_SPAN.copysign(increment as f32);
            let slope = |from: f32, to: f32| {
                let rise = read_at(to.rem_euclid(1.0)) - read_at(from.rem_euclid(1.0));
                (rise / span * increment as f32).clamp(-PI, PI)
            };
            // At the cycle's start the accumulator is 0, so the effective
            // read phase is the offset itself.
            let frac = reset.frac();
            let reached = (raw_phase as f64 + increment * frac as f64).rem_euclid(1.0) as f32;
            let (now, carry) = sync_corner(
                read_at(read_offset) - read_at(reached),
                slope(read_offset, read_offset + span) - slope(reached - span, reached),
                frac,
                increment as f32,
            );
            sample += now;
            self.blep_carry = carry;
            next = (increment * (1.0 - frac as f64)).rem_euclid(1.0);
        }

        self.phase = next;
        // Wavetables are normalized in [-1, 1] after the FFT round-trip;
        // scale to the synth's audio range.
        sample * 5.0
    }
}

/// The span, in cycles, of the difference a reset measures the table's slope
/// across.
const SLOPE_SPAN: f32 = 1.0 / 4096.0;

/// Derive the lane count from the maximum of `pitch` and (optional)
/// `position`. Clamped to `[1, PORT_MAX_LANES]`.
#[allow(private_interfaces)]
pub fn wavetable_derive_lane_count(params: &WavetableOscParams) -> usize {
    let pitch_ch = params.pitch.lanes();
    let pos_ch = params.position.as_ref().map(|p| p.lanes()).unwrap_or(0);
    let fm_ch = params.fm.as_ref().map(|f| f.lanes()).unwrap_or(0);
    let phase_ch = params.phase.as_ref().map(|t| t.lanes()).unwrap_or(0);
    let sync_ch = params.sync.as_ref().map(|s| s.lanes()).unwrap_or(0);
    let restart_ch = params.restart.as_ref().map(|r| r.lanes()).unwrap_or(0);
    let offset_ch = params.phase_offset.as_ref().map(|o| o.lanes()).unwrap_or(0);
    pitch_ch
        .max(pos_ch)
        .max(fm_ch)
        .max(phase_ch)
        .max(sync_ch)
        .max(restart_ch)
        .max(offset_ch)
        .clamp(1, PORT_MAX_LANES)
}

const WAVETABLE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Wavetable oscillator",
    description: "Plays the single-cycle frames of a wavetable WAV and blends \
                  between them, for evolving glassy, vocal or digital timbres. \
                  Sweep `position` with an LFO or envelope, and bend each cycle \
                  further with a `$phase.*` warp. Tables bundled with the app \
                  live under `'builtin/tables/'`.",
    categories: &[ReferenceCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "Scanning pad",
            code: r#"
                chord: $wavetable(
                  'c3:m9',
                  'builtin/tables/mi/formants.wav',
                  $sine('0.1hz').range(0, 5),
                )
            "#,
        },
        DocExample {
            title: "Plucked table sweep",
            code: r#"
                const seq = $`c3 eb3 g3 bb3`
                const env = $perc(seq, { decay: 0.5 })
                melody: $wavetable(
                  seq,
                  'builtin/tables/mi/fm-bell.wav',
                  env.range(0, 4),
                ).amp(env)
            "#,
        },
    ],
};

#[module(
    name = "$wavetable", docs = WAVETABLE_DOCS,
    lanes_derive = wavetable_derive_lane_count,
    args(pitch, wav, position),
    has_prepare_resources,
    patch_update,
    block,
)]
pub struct WavetableOsc {
    params: WavetableOscParams,
    outputs: WavetableOscOutputs,
    lane_state: Box<[LaneState]>,
}

impl WavetableOsc {
    fn update(&mut self, sample_rate: f32) {
        let lanes = self.lane_count();
        let Some(prepared) = self.params.prepared.as_ref().filter(|p| p.is_playable()) else {
            // Silent fallback — prepared mipmap is missing (wav unloaded / empty).
            for lane in 0..lanes {
                self.outputs.sample.set(lane, 0.0);
            }
            return;
        };
        let inv_sr = 1.0 / sample_rate as f64;
        let p = &self.params;
        for (lane, state) in self.lane_state.iter_mut().enumerate() {
            let ratio = p.sync.as_ref().map(|sync| sync_ratio(sync.get_value(lane)));
            state.tune(
                prepared,
                p.pitch.get_value(lane),
                p.fm.value_or(lane, 0.0),
                p.fm_mode,
                ratio,
            );
            let resolved = p.phase.as_ref().map(|warp| {
                (
                    warp,
                    warp.resolve(lane, sample_rate, &mut state.warp_glides),
                )
            });
            let sample = state.step(
                prepared,
                sample_rate,
                inv_sr,
                p.position
                    .value_or(lane, WavetableOscParams::POSITION_DEFAULT),
                p.phase_offset
                    .value_or(lane, WavetableOscParams::PHASE_OFFSET_DEFAULT),
                resolved.as_ref().map(|(warp, resolved)| (*warp, resolved)),
                p.restart.as_ref().map(|restart| restart.get_value(lane)),
                ratio,
            );
            self.outputs.sample.set(lane, sample);
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut WavetableOscBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let p = &self.params;
        // A warp reads its own signals and glides per sample: leave it to
        // `update`.
        if p.phase.is_some() {
            return None;
        }
        let out = &mut outputs.sample;
        let Some(prepared) = p.prepared.as_ref().filter(|p| p.is_playable()) else {
            for i in range {
                out.slot_mut(i).fill(0.0);
            }
            return Some(());
        };
        let lanes = self.lane_count();
        let mut scratch = Default::default();
        let WavetableOscParamsBlockInputs {
            pitch,
            position,
            fm,
            sync,
            restart,
            phase_offset: offset,
        } = p.block_inputs(range.clone(), lanes, &mut scratch)?;
        let inv_sr = 1.0 / sample_rate as f64;
        let mut ratios = SyncRatios::new(sync, range.start);
        for i in range {
            let (pitch, fm, position, offset) =
                (pitch.row(i), fm.row(i), position.row(i), offset.row(i));
            let restart = restart.as_ref().map(|restart| restart.row(i));
            let ratios = ratios.at(i);
            let slots = out.slot_mut(i).iter_mut().zip(self.lane_state.iter_mut());
            for (lane, (out, state)) in slots.enumerate() {
                state.tune(prepared, pitch[lane], fm[lane], p.fm_mode, ratios[lane]);
                let sample = state.step(
                    prepared,
                    sample_rate,
                    inv_sr,
                    position[lane],
                    offset[lane],
                    None,
                    restart.map(|restart| restart[lane]),
                    ratios[lane],
                );
                *out = sanitize(sample);
            }
        }
        Some(())
    }

    /// Main-thread resource preparation. Builds the mipmap pyramid from the
    /// referenced WAV if it's present in the cache snapshot.
    fn prepare_resources_impl(
        &mut self,
        wav_data: &HashMap<String, Arc<WavData>>,
        sample_rate: f32,
    ) {
        if let Some(wd) = wav_data.get(self.params.wav.path()) {
            self.params.prepared = Some(PreparedWavetable::from_wav_data(wd, sample_rate));
        } else {
            self.params.prepared = None;
        }
    }
}

impl crate::types::PatchUpdateHandler for WavetableOsc {
    fn on_patch_update(&mut self) {
        // The cached frequency and levels also depend on `fmMode` and the table,
        // either of which a patch update can change.
        for state in self.lane_state.iter_mut() {
            state.last_pitch = f32::NAN;
        }
    }
}

message_handlers!(impl WavetableOsc {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, SampleBuffer, Signal};
    use std::f32::consts::TAU;

    /// Build a WavData containing `n_frames` sine-wave frames of `size`
    /// samples each.
    fn make_wav(size: usize, n_frames: usize) -> Arc<WavData> {
        let mut channel = Vec::with_capacity(size * n_frames);
        for f in 0..n_frames {
            // Vary the waveform per frame: frame 0 = sine, frame 1 = cosine,
            // frame 2 = inverted sine, etc.
            let phase_offset = f as f32 * TAU * 0.25;
            for i in 0..size {
                let p = TAU * (i as f32 / size as f32) + phase_offset;
                channel.push(p.sin());
            }
        }
        Arc::new(WavData::new(
            SampleBuffer::from_samples(vec![channel], 48000.0),
            Some(size),
        ))
    }

    fn make_osc(params: WavetableOscParams) -> WavetableOsc {
        let lanes = wavetable_derive_lane_count(&params);
        let mut outputs = WavetableOscOutputs::default();
        outputs.set_all_lanes(lanes);

        WavetableOsc {
            params,
            outputs,
            lane_state: vec![LaneState::default(); lanes].into_boxed_slice(),
            _lane_count: lanes,
            _channels: 1,
            _block_index: Default::default(),
            _connected: Default::default(),
        }
    }

    fn base_params() -> WavetableOscParams {
        WavetableOscParams {
            wav: Wav::new("test.wav".to_string(), 1),
            pitch: PolySignal::mono(Signal::Volts(0.0)),
            position: None,
            fm: None,
            fm_mode: FmMode::default(),
            sync: None,
            restart: None,
            phase: None,
            phase_offset: None,
            prepared: None,
        }
    }

    /// The cached frequency follows a patch update that changes `fmMode` while
    /// the pitch and FM inputs stay the same.
    #[test]
    fn fm_mode_change_refreshes_cached_frequency() {
        use crate::types::PatchUpdateHandler;

        let mut params = base_params();
        params.fm = Some(PolySignal::mono(Signal::Volts(0.5)));
        params.fm_mode = FmMode::Exp;
        params.prepared = Some(PreparedWavetable::from_wav_data(&make_wav(256, 1), 48000.0));
        let mut osc = make_osc(params);
        osc.update(48000.0);
        assert_eq!(osc.lane_state[0].freq, apply_fm(0.0, 0.5, FmMode::Exp));

        osc.params.fm_mode = FmMode::Lin;
        osc.on_patch_update();
        osc.update(48000.0);
        assert_eq!(osc.lane_state[0].freq, apply_fm(0.0, 0.5, FmMode::Lin));
        assert_ne!(
            apply_fm(0.0, 0.5, FmMode::Lin),
            apply_fm(0.0, 0.5, FmMode::Exp)
        );
    }

    #[test]
    fn silent_when_not_prepared() {
        let mut osc = make_osc(base_params());
        for _ in 0..128 {
            osc.update(48000.0);
        }
        // No panic, no audio.
        assert_eq!(osc.outputs.sample.get(0), 0.0);
    }

    #[test]
    fn produces_sine_shaped_output_at_c4() {
        let size = 2048;
        let wav = make_wav(size, 1);
        let mut params = base_params();
        params.prepared = Some(PreparedWavetable::from_wav_data(&wav, 48000.0));
        let mut osc = make_osc(params);

        // At C4 (0V), freq ≈ 261.63 Hz; at 48kHz that's ~183 samples/cycle.
        // Run ~4 cycles and track min/max plus RMS.
        let n = 800;
        let mut samples = Vec::with_capacity(n);
        for _ in 0..n {
            osc.update(48000.0);
            samples.push(osc.outputs.sample.get(0));
        }
        let max = samples.iter().cloned().fold(f32::MIN, f32::max);
        let min = samples.iter().cloned().fold(f32::MAX, f32::min);
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();

        // A unit-amplitude sine scaled by 5 should oscillate in about ±5V.
        assert!(max > 4.0, "expected peak near +5V, got {max}");
        assert!(min < -4.0, "expected trough near -5V, got {min}");
        // RMS of a ±5V sine is 5/sqrt(2) ≈ 3.54V.
        assert!(
            (rms - 5.0 / 2.0_f32.sqrt()).abs() < 0.5,
            "expected RMS near 3.54, got {rms}"
        );
    }

    #[test]
    fn position_selects_different_frames() {
        let size = 2048;
        let wav = make_wav(size, 2);
        // Frame 0 = sin(phase); frame 1 = sin(phase + 0.25*TAU) = cos(phase).
        // Play a fixed phase (freq ≈ 0) and sweep position between frames.

        let mut p0 = base_params();
        p0.pitch = PolySignal::mono(Signal::Volts(-10.0)); // very low freq
        p0.position = Some(PolySignal::mono(Signal::Volts(0.0))); // frame 0
        p0.prepared = Some(PreparedWavetable::from_wav_data(&wav, 48000.0));
        let mut osc0 = make_osc(p0);

        let mut p1 = base_params();
        p1.pitch = PolySignal::mono(Signal::Volts(-10.0));
        p1.position = Some(PolySignal::mono(Signal::Volts(5.0))); // frame 1
        p1.prepared = Some(PreparedWavetable::from_wav_data(&wav, 48000.0));
        let mut osc1 = make_osc(p1);

        // Run one sample each (phase still ~0, so frame 0 ≈ 0, frame 1 ≈ 1*5).
        osc0.update(48000.0);
        osc1.update(48000.0);

        let s0 = osc0.outputs.sample.get(0);
        let s1 = osc1.outputs.sample.get(0);
        // Frame 0 sine at phase 0 → ~0. Frame 1 cosine at phase 0 → ~1*5V.
        assert!(s0.abs() < 0.5, "frame 0 start ~0, got {s0}");
        assert!((s1 - 5.0).abs() < 0.5, "frame 1 start ~5V, got {s1}");
    }

    #[test]
    fn phase_warp_changes_output() {
        let size = 2048;
        let wav = make_wav(size, 1);

        let mut p_plain = base_params();
        p_plain.pitch = PolySignal::mono(Signal::Volts(0.0));
        p_plain.prepared = Some(PreparedWavetable::from_wav_data(&wav, 48000.0));
        let mut osc_plain = make_osc(p_plain);

        let mut p_warped = base_params();
        p_warped.pitch = PolySignal::mono(Signal::Volts(0.0));
        p_warped.phase = Some(PhaseWarp::Mirror {
            amount: PolySignal::mono(Signal::Volts(3.0)),
        });
        p_warped.prepared = Some(PreparedWavetable::from_wav_data(&wav, 48000.0));
        let mut osc_warped = make_osc(p_warped);

        // Run enough samples for phase to drift across cycles.
        let n = 400;
        let mut plain = Vec::with_capacity(n);
        let mut warped = Vec::with_capacity(n);
        for _ in 0..n {
            osc_plain.update(48000.0);
            osc_warped.update(48000.0);
            plain.push(osc_plain.outputs.sample.get(0));
            warped.push(osc_warped.outputs.sample.get(0));
        }
        let diff: f32 = plain
            .iter()
            .zip(warped.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        // If the warp had no effect, diff would be ~0.
        assert!(
            diff > 50.0,
            "expected warped output to differ; diff = {diff}"
        );
    }

    #[test]
    fn prepare_resources_noop_when_wav_missing() {
        let mut osc = make_osc(base_params());
        let wav_data: HashMap<String, Arc<WavData>> = HashMap::new();
        osc.prepare_resources_impl(&wav_data, 48000.0);
        assert!(osc.params.prepared.is_none());
        osc.update(48000.0);
        assert_eq!(osc.outputs.sample.get(0), 0.0);
    }

    #[test]
    fn prepare_resources_populates_when_wav_present() {
        let size = 2048;
        let wav = make_wav(size, 1);
        let mut osc = make_osc(base_params());
        let mut wav_data: HashMap<String, Arc<WavData>> = HashMap::new();
        wav_data.insert("test.wav".to_string(), wav);
        osc.prepare_resources_impl(&wav_data, 48000.0);
        assert!(osc.params.prepared.is_some());
        // Processing should now produce non-zero samples.
        let mut any_nonzero = false;
        for _ in 0..400 {
            osc.update(48000.0);
            if osc.outputs.sample.get(0).abs() > 0.1 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output once prepared");
    }

    fn prepared(sync: Option<f32>) -> WavetableOsc {
        let mut osc = make_osc(WavetableOscParams {
            pitch: PolySignal::mono(Signal::Volts(2.0)),
            sync: sync.map(|v| PolySignal::mono(Signal::Volts(v))),
            ..base_params()
        });
        let mut wav_data: HashMap<String, Arc<WavData>> = HashMap::new();
        wav_data.insert("test.wav".to_string(), make_wav(2048, 1));
        osc.prepare_resources_impl(&wav_data, 48000.0);
        osc
    }

    /// Sync at 0 restarts each cycle exactly where it wraps anyway, so from
    /// the first sample, across restarts and with sync switched on partway,
    /// the wave is the free one.
    #[test]
    fn sync_at_zero_is_the_free_wave() {
        let render = |sync_from: Option<usize>| {
            let mut osc = prepared(None);
            (0..4000)
                .map(|i| {
                    osc.params.restart = Some(PolySignal::mono(Signal::Volts(if i % 1000 < 2 {
                        5.0
                    } else {
                        0.0
                    })));
                    if sync_from == Some(i) {
                        osc.params.sync = Some(PolySignal::mono(Signal::Volts(0.0)));
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
    fn sync_reads_the_table_at_the_synced_pitch() {
        let mut osc = prepared(Some(2.0));
        osc.update(48000.0);
        let state = osc.lane_state[0];
        let prepared = osc.params.prepared.as_ref().unwrap();
        assert_eq!(state.mip, prepared.mipmap_for_freq(state.freq * 4.0));
    }

    #[test]
    fn restart_starts_the_cycle_where_the_trigger_crosses() {
        // A trigger from 0 to 5 V crosses its threshold a fifth of the way
        // through the step the restart lands in, so the next read stands
        // four fifths of a step into the new cycle.
        let mut osc = prepared(None);
        osc.params.restart = Some(PolySignal::mono(Signal::Volts(0.0)));
        for _ in 0..700 {
            osc.update(48000.0);
        }
        osc.params.restart = Some(PolySignal::mono(Signal::Volts(5.0)));
        osc.update(48000.0);
        let state = osc.lane_state[0];
        let step = state.freq as f64 / 48000.0;
        assert!((state.phase - 0.8 * step).abs() < 1e-6, "{}", state.phase);
    }

    #[test]
    fn a_restart_rounds_the_corner_it_makes() {
        // Restarted halfway through a step as its sine table falls through
        // zero, the wave turns sharply upward from the cycle's start: the
        // samples either side of the corner rise by the same PolyBLAMP share.
        let mut osc = prepared(None);
        osc.params.pitch = PolySignal::mono(Signal::Volts(0.0));
        osc.params.restart = Some(PolySignal::mono(Signal::Volts(0.0)));
        osc.update(48000.0);
        let step = osc.lane_state[0].freq as f64 / 48000.0;
        osc.lane_state[0].phase = 0.5 - 0.5 * step;
        // From 0 to 2 V, the trigger crosses its threshold halfway.
        osc.params.restart = Some(PolySignal::mono(Signal::Volts(2.0)));
        let state = osc.lane_state[0];
        let prepared = osc.params.prepared.as_ref().unwrap();
        let table = |phase: f64| 5.0 * prepared.read(state.mip, 0.0, phase as f32);
        let (naive_first, naive_second) = (table(0.5 - 0.5 * step), table(0.5 * step));
        osc.update(48000.0);
        let first = osc.outputs.sample.get(0);
        osc.update(48000.0);
        let second = osc.outputs.sample.get(0);
        let fade = (1.0 - 2.0 * step).powi(2);
        let share = (5.0 * 2.0 * std::f64::consts::TAU * step / 6.0 * 0.125 * fade) as f32;
        let (lift_first, lift_second) = (first - naive_first, second - naive_second);
        assert!(
            (lift_first - share).abs() < 0.05 * share,
            "{lift_first} vs {share}"
        );
        assert!(
            (lift_second - share).abs() < 0.05 * share,
            "{lift_second} vs {share}"
        );
    }

    #[test]
    fn a_warp_that_jumps_at_the_cycle_start_keeps_resets_in_range() {
        // A partial mirror jumps where the raw phase wraps, right at the
        // cycle's start. A reset measures the slope on the side the wave
        // travels, never across that jump.
        for (sync, restart_every) in [(Some(1.37), None), (Some(0.6), None), (None, Some(97))] {
            let mut osc = prepared(sync);
            osc.params.phase = Some(PhaseWarp::Mirror {
                amount: PolySignal::mono(Signal::Volts(3.0)),
            });
            for i in 0..8000 {
                if let Some(every) = restart_every {
                    let v = if i % every < 2 { 5.0 } else { 0.0 };
                    osc.params.restart = Some(PolySignal::mono(Signal::Volts(v)));
                }
                osc.update(48000.0);
                let v = osc.outputs.sample.get(0);
                assert!(
                    v.abs() <= 5.5,
                    "sync {sync:?}, restart {restart_every:?}, sample {i}: {v}"
                );
            }
        }
    }
}
