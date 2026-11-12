//! One DX7 voice: Plaits' `FMVoice` and `fm::Voice<6>`, rendered one sample
//! at a time.

use super::super::core::sine::{phase_increment, sine_pm};
use super::algorithms::{Call, Kind};
use super::envelope::{Envelope, render_at_sample};
use super::lfo::Lfo;
use super::patch::Patch;
use super::units::{keyboard_scaling, normalize_velocity, pow2_fast_1, pow2_fast_2, rate_scaling};

/// Rate-dependent constants every voice shares.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rates {
    pub sample_rate: f32,
    /// One Hz in cycles per sample.
    pub one_hz: f32,
    /// The envelopes' 44.1 kHz increments to this rate.
    pub envelope_scale: f32,
    /// How long the scrubbed envelope holds its gate, in samples.
    pub gate_duration: f32,
}

impl Rates {
    pub fn new(sample_rate: f32) -> Self {
        let one_hz = 1.0 / sample_rate;
        Self {
            sample_rate,
            one_hz,
            envelope_scale: 44100.0 * one_hz,
            gate_duration: 1.5 * sample_rate,
        }
    }
}

/// `fm::Voice<6>::Parameters`, with the pitch as a frequency.
#[derive(Clone, Copy, Debug)]
pub struct Parameters {
    /// Scrubs the envelopes to `envelope_control` rather than running them.
    pub sustain: bool,
    pub gate: bool,
    /// The note's frequency, in cycles per sample; negative runs the ratio
    /// operators backward.
    pub f0: f32,
    pub velocity: f32,
    pub brightness: f32,
    pub envelope_control: f32,
    /// LFO pitch modulation, in octaves.
    pub pitch_mod: f32,
    pub amp_mod: f32,
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            sustain: false,
            gate: false,
            f0: 0.0,
            velocity: 0.5,
            brightness: 0.5,
            envelope_control: 0.5,
            pitch_mod: 0.0,
            amp_mod: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Operator {
    phase: u32,
    amplitude: f32,
}

/// Values a voice derives from its inputs, recomputed only when those inputs
/// change, bit for bit. `NaN` keys force the first computation, and a new
/// patch clears every key.
#[derive(Clone, Copy, Debug)]
struct Cache {
    /// The patch (by address) everything below is for.
    patch: usize,
    /// The input frequency, and its note number.
    note_f0: f32,
    note_number: f32,
    /// The pitch modulation, and 2^`pitch_mod`.
    pitch_mod: f32,
    pitch_ratio: f32,
    /// The modulated frequency, and each operator's phase increment.
    f0: f32,
    increment: [u32; 6],
    /// The envelope control, and the envelope scales and scrub point.
    envelope_control: f32,
    ad_scale: f32,
    r_scale: f32,
    envelope_sample: f32,
    /// The note, and each operator's envelope rate scaling.
    rate_note: f32,
    rate_scaling: [f32; 6],
    /// The note, velocity and brightness, and the level offset they give each
    /// operator.
    offset_note: f32,
    offset_velocity: f32,
    offset_brightness: f32,
    offset: [f32; 6],
    /// The amplitude modulation, and each operator's level scale.
    amp_mod: f32,
    level_mod: [f32; 6],
    /// Each operator's level, and its amplitude.
    level: [f32; 6],
    amplitude: [f32; 6],
    /// The scrub point, and the levels there.
    scrub_sample: f32,
    scrubbed: [f32; 6],
    scrubbed_pitch: f32,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            patch: 0,
            note_f0: f32::NAN,
            note_number: 48.0,
            pitch_mod: f32::NAN,
            pitch_ratio: 1.0,
            f0: f32::NAN,
            increment: [0; 6],
            envelope_control: f32::NAN,
            ad_scale: 1.0,
            r_scale: 1.0,
            envelope_sample: 0.0,
            rate_note: f32::NAN,
            rate_scaling: [1.0; 6],
            offset_note: f32::NAN,
            offset_velocity: f32::NAN,
            offset_brightness: f32::NAN,
            offset: [0.0; 6],
            amp_mod: f32::NAN,
            level_mod: [1.0; 6],
            level: [f32::NAN; 6],
            amplitude: [0.0; 6],
            scrub_sample: f32::NAN,
            scrubbed: [0.0; 6],
            scrubbed_pitch: 0.0,
        }
    }
}

/// Whether `a` and `b` differ bit for bit.
#[inline(always)]
fn moved(a: f32, b: f32) -> bool {
    a.to_bits() != b.to_bits()
}

#[derive(Clone, Copy, Debug)]
pub struct FmVoice {
    patch: Option<&'static Patch>,
    pub lfo: Lfo,
    pub parameters: Parameters,
    gate: bool,
    operators: [Operator; 6],
    envelopes: [Envelope; 6],
    pitch_envelope: Envelope,
    normalized_velocity: f32,
    note: f32,
    feedback_state: [f32; 2],
    cache: Cache,
}

impl Default for FmVoice {
    fn default() -> Self {
        Self {
            patch: None,
            lfo: Lfo::default(),
            parameters: Parameters::default(),
            gate: false,
            operators: [Operator::default(); 6],
            envelopes: [Envelope::default(); 6],
            pitch_envelope: Envelope::default(),
            normalized_velocity: 10.0,
            note: 48.0,
            feedback_state: [0.0; 2],
            cache: Cache::default(),
        }
    }
}

/// The MIDI note number of `f0` cycles per sample, within Plaits' note range.
pub fn midi_note(f0: f32, sample_rate: f32) -> f32 {
    // f64 then rounded, so every platform's libm gives the same value.
    let note = 69.0 + 12.0 * (f64::from(f0.abs()) * f64::from(sample_rate) / 440.0).log2();
    (note as f32).clamp(-119.0, 120.0)
}

/// Plaits' `RenderOperators` for one sample: a chain of `N` operators, each
/// phase-modulating the next, starting from `pm`. `SOURCE` is the operator
/// whose output feeds back, if any.
#[inline(always)]
fn render_chain<const N: usize, const SOURCE: i32>(
    operators: &mut [Operator; 6],
    first: usize,
    increment: &[u32; 6],
    amplitude: &[f32; 6],
    feedback_state: &mut [f32; 2],
    mut pm: f32,
) -> f32 {
    for k in 0..N {
        let op = &mut operators[first + k];
        op.phase = op.phase.wrapping_add(increment[first + k]);
        pm = sine_pm(op.phase, pm) * op.amplitude;
        op.amplitude += amplitude[first + k] - op.amplitude;
        if k as i32 == SOURCE {
            feedback_state[1] = feedback_state[0];
            feedback_state[0] = pm;
        }
    }
    pm
}

impl FmVoice {
    /// `FMVoice::LoadPatch`: a different patch takes over the operators and
    /// LFO settings, keeping their phases and the envelopes' stages.
    pub fn load_patch(&mut self, patch: &'static Patch, rates: &Rates) {
        if self.patch.is_some_and(|p| std::ptr::eq(p, patch)) {
            return;
        }
        self.patch = Some(patch);
        self.lfo.set(&patch.lfo, rates.one_hz);
    }

    pub fn set_modulations(&mut self, lfo: &Lfo) {
        self.parameters.pitch_mod = lfo.pitch_mod();
        self.parameters.amp_mod = lfo.amp_mod();
    }

    /// One sample of the voice; silent until a patch loads.
    #[inline(always)]
    pub fn render(&mut self, rates: &Rates) -> f32 {
        let Some(patch) = self.patch else {
            return 0.0;
        };
        let p = self.parameters;
        let patch_id = patch as *const Patch as usize;
        if patch_id != self.cache.patch {
            self.cache = Cache {
                patch: patch_id,
                ..Cache::default()
            };
        }
        let cache = &mut self.cache;
        let scale = rates.envelope_scale;
        let gate_duration = rates.gate_duration;

        if moved(p.envelope_control, cache.envelope_control) {
            cache.envelope_control = p.envelope_control;
            cache.ad_scale = pow2_fast_1((0.5 - p.envelope_control) * 8.0);
            cache.r_scale = pow2_fast_1(-(p.envelope_control - 0.3).abs() * 8.0);
            cache.envelope_sample = gate_duration * p.envelope_control;
        }
        let (ad_scale, r_scale) = (cache.ad_scale, cache.r_scale);

        if p.sustain && moved(cache.envelope_sample, cache.scrub_sample) {
            let t = cache.envelope_sample;
            cache.scrub_sample = t;
            cache.scrubbed_pitch =
                render_at_sample::<false>(&patch.pitch_envelope, scale, t, gate_duration);
            for (level, op) in cache.scrubbed.iter_mut().zip(&patch.op) {
                *level = render_at_sample::<true>(&op.envelope, scale, t, gate_duration);
            }
        }

        let pitch_envelope = if p.sustain {
            cache.scrubbed_pitch
        } else {
            self.pitch_envelope.render::<false>(
                &patch.pitch_envelope,
                scale,
                p.gate,
                1.0,
                ad_scale,
                r_scale,
            )
        };
        let pitch_mod = pitch_envelope + p.pitch_mod;
        if moved(pitch_mod, cache.pitch_mod) {
            cache.pitch_mod = pitch_mod;
            // f64 then rounded, so every platform's libm gives the same value.
            cache.pitch_ratio = f64::from(pitch_mod).exp2() as f32;
        }
        let f0 = p.f0 * cache.pitch_ratio;
        if moved(f0, cache.f0) {
            cache.f0 = f0;
            for (increment, op) in cache.increment.iter_mut().zip(&patch.op) {
                let f = op.ratio * if op.ratio < 0.0 { -rates.one_hz } else { f0 };
                *increment = phase_increment(f.clamp(-0.5, 0.5));
            }
        }

        // Note and velocity scale the levels from the note-on on, or follow
        // their inputs when scrubbed.
        let note_on = p.gate && !self.gate;
        self.gate = p.gate;
        if note_on || p.sustain {
            self.normalized_velocity = normalize_velocity(p.velocity);
            if moved(p.f0, cache.note_f0) {
                cache.note_f0 = p.f0;
                cache.note_number = midi_note(p.f0, rates.sample_rate);
            }
            self.note = cache.note_number;
        }
        if note_on && patch.reset_phase {
            for op in &mut self.operators {
                op.phase = 0;
            }
        }
        if !p.sustain && moved(self.note, cache.rate_note) {
            cache.rate_note = self.note;
            for (scaling, op) in cache.rate_scaling.iter_mut().zip(&patch.op) {
                *scaling = rate_scaling(self.note, op.rate_scaling);
            }
        }
        if moved(self.note, cache.offset_note)
            || moved(self.normalized_velocity, cache.offset_velocity)
            || moved(p.brightness, cache.offset_brightness)
        {
            cache.offset_note = self.note;
            cache.offset_velocity = self.normalized_velocity;
            cache.offset_brightness = p.brightness;
            let brightness = (p.brightness - 0.5) * 32.0;
            for (i, op) in patch.op.iter().enumerate() {
                let keyboard = keyboard_scaling(self.note, &op.keyboard_scaling);
                let velocity = self.normalized_velocity * op.velocity_sensitivity;
                let brightness = if patch.algorithm.is_modulator[i] {
                    brightness
                } else {
                    0.0
                };
                cache.offset[i] = 0.125 * (keyboard + velocity + brightness).min(op.level_headroom);
            }
        }
        if moved(p.amp_mod, cache.amp_mod) {
            cache.amp_mod = p.amp_mod;
            for (level_mod, op) in cache.level_mod.iter_mut().zip(&patch.op) {
                let log_level_mod = op.amp_mod_sensitivity * p.amp_mod - 1.0;
                *level_mod = 1.0 - pow2_fast_2(6.4 * log_level_mod);
            }
            // The amplitudes follow the new scales.
            cache.level = [f32::NAN; 6];
        }

        for (i, op) in patch.op.iter().enumerate() {
            let envelope = if p.sustain {
                cache.scrubbed[i]
            } else {
                self.envelopes[i].render::<true>(
                    &op.envelope,
                    scale,
                    p.gate,
                    cache.rate_scaling[i],
                    ad_scale,
                    r_scale,
                )
            };
            let level = envelope + cache.offset[i];
            if moved(level, cache.level[i]) {
                cache.level[i] = level;
                cache.amplitude[i] = pow2_fast_2(-14.0 + level * cache.level_mod[i]).min(4.0);
            }
        }
        let increment = cache.increment;
        let amplitude = cache.amplitude;

        let mut buffers = [0.0; 4];
        let feedback_scale = patch.feedback_scale;
        for &Call {
            kind,
            first,
            input,
            output,
        } in patch.algorithm.calls()
        {
            let ops = &mut self.operators;
            let fb = &mut self.feedback_state;
            let (sample, additive) = match kind {
                Kind::External { additive } => (
                    render_chain::<1, -2>(ops, first, &increment, &amplitude, fb, buffers[input]),
                    additive,
                ),
                Kind::Unmodulated { additive } => (
                    render_chain::<1, -1>(ops, first, &increment, &amplitude, fb, 0.0),
                    additive,
                ),
                Kind::Feedback { additive } => {
                    let pm = (fb[0] + fb[1]) * feedback_scale;
                    (
                        render_chain::<1, 0>(ops, first, &increment, &amplitude, fb, pm),
                        additive,
                    )
                }
                Kind::Loop3 => {
                    let pm = (fb[0] + fb[1]) * feedback_scale;
                    (
                        render_chain::<3, 2>(ops, first, &increment, &amplitude, fb, pm),
                        true,
                    )
                }
                Kind::Loop2 => {
                    let pm = (fb[0] + fb[1]) * feedback_scale;
                    (
                        render_chain::<2, 1>(ops, first, &increment, &amplitude, fb, pm),
                        true,
                    )
                }
            };
            if additive {
                buffers[output] += sample;
            } else {
                buffers[output] = sample;
            }
        }
        buffers[0]
    }
}
