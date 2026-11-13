//! MIDI to CV converter module with polyphonic voice allocation.
//!
//! Converts MIDI note messages to pitch CV and gate signals with configurable
//! voice allocation modes following VCV Rack conventions.

use deserr::Deserr;
use napi::Result;
use schemars::JsonSchema;
use serde::Serialize;

use crate::dsp::utils::{
    GATE_HIGH_VOLTAGE, GATE_LOW_VOLTAGE, TempGate, TempGateState, min_gate_samples,
};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PORT_MAX_LANES, PolyOutput};
use crate::types::{
    DeviceName, MidiChannelPressure, MidiControlChange, MidiNoteOff, MidiNoteOn, MidiPitchBend,
    MidiPolyPressure,
};

/// Voice allocation mode for polyphonic operation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, Serialize, JsonSchema, Connect)]
#[serde(rename_all = "lowercase")]
#[deserr(rename_all = lowercase)]
pub enum PolyMode {
    /// each new note takes the next voice in turn, so release tails ring on
    #[default]
    Rotate,
    /// a repeated note retriggers the voice already playing it, otherwise takes the next voice
    Reuse,
    /// each new note takes the lowest free voice
    Reset,
    /// MPE: each MIDI channel drives its own voice, with per-note bends and pressure
    Mpe,
}

/// Note priority for monophonic operation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserr, Serialize, JsonSchema, Connect)]
#[serde(rename_all = "lowercase")]
#[deserr(rename_all = lowercase)]
pub enum MonoMode {
    /// the newest note you play takes over
    #[default]
    Last,
    /// the first held note keeps playing until you let go
    First,
    /// the lowest held note plays, for bass lines
    Lowest,
    /// the highest held note plays, for leads
    Highest,
}

/// State for a single voice
#[derive(Debug, Clone, Copy, Default)]
struct VoiceState {
    /// MIDI note number (0-127)
    note: u8,
    /// Velocity (0-127)
    velocity: u8,
    /// Gate state
    gate: bool,
    /// Aftertouch (0-127)
    aftertouch: u8,
    /// Pitch wheel value (-8192 to 8191)
    pitch_wheel: i16,
    /// Mod wheel value (0-127)
    mod_wheel: u8,
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MidiCvParams {
    #[input("MIDI device to listen to; every device when unset")]
    #[deserr(default)]
    device: Option<String>,

    #[input("number of voices", range = (1, 64))]
    #[serde(default = "default_lanes")]
    #[deserr(default = default_lanes())]
    lanes: usize,

    #[input("MIDI channel to listen to; every channel when unset", range = (1, 16))]
    #[deserr(default)]
    channel: Option<u8>,

    #[input("how new notes are given to voices")]
    #[serde(default)]
    #[deserr(default)]
    poly_mode: PolyMode,

    #[input("which held note plays when `lanes` is 1")]
    #[serde(default)]
    #[deserr(default)]
    mono_mode: MonoMode,

    #[input("how far the pitch wheel bends, in semitones; 0 turns bending off")]
    #[serde(default = "default_pitch_bend_range")]
    #[deserr(default = default_pitch_bend_range())]
    pitch_bend_range: u8,
}

fn default_lanes() -> usize {
    1
}

fn default_pitch_bend_range() -> u8 {
    2
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MidiCvOutputs {
    #[output("pitch", "pitch of the voice's note, including pitch bend", type = cv, default)]
    pitch: PolyOutput,
    #[output("gate", "high while the voice's note is held", type = gate)]
    gate: PolyOutput,
    #[output("velocity", "how hard the note was struck", type = cv, range = (0.0, 5.0))]
    velocity: PolyOutput,
    #[output("aftertouch", "pressure on the held keys, from channel or polyphonic aftertouch", type = cv, range = (0.0, 5.0))]
    aftertouch: PolyOutput,
    #[output("retrigger", "fires at the start of each new note, even when gate stays high", type = trig)]
    retrigger: PolyOutput,
    #[output("pitchWheel", "pitch wheel position: 0 centred, ±5 at either end", type = cv, range = (-5.0, 5.0))]
    pitch_wheel: PolyOutput,
    #[output("modWheel", "mod wheel (CC 1) position", type = cv, range = (0.0, 5.0))]
    mod_wheel: PolyOutput,
}

const MIDI_CV_DOCS: ModuleDocs = ModuleDocs {
    summary: "MIDI notes to pitch and gate",
    description: "Turns notes from a MIDI keyboard or controller into pitch, gate, velocity and \
                  expression voltages for playing a patch live. Set `lanes` above 1 for a \
                  polyphonic synth, one voice per lane, or leave it at 1 for a mono lead.",
    categories: &[ModuleCategory::Midi],
    sections: &[],
    examples: &[
        DocExample {
            title: "Four-voice keyboard synth",
            code: r#"
                const midi = $midiCV({ lanes: 4 })
                $saw(midi)
                  .lpf(midi._velocity.range('c4', 'c7'), 1)
                  .amp($adsr(midi, { release: 0.4 }))
                  .out()
            "#,
        },
        DocExample {
            title: "Mono lead with a wide pitch bend",
            code: r#"
                const lead = $midiCV({ pitchBendRange: 12 })
                $pulse(lead)
                  .lpf(lead._modWheel.range('c4', 'c7'), 2)
                  .amp($adsr(lead))
                  .out()
            "#,
        },
    ],
};

#[module(
    name = "$midiCV",
    docs = MIDI_CV_DOCS,
    lanes_param = "lanes",
    args()
)]
pub struct MidiCv {
    outputs: MidiCvOutputs,
    params: MidiCvParams,
    state: MidiCvState,
    /// Per-voice state, one element per lane.
    lane_state: Box<[MidiCvLane]>,
}

/// Per-voice state (one element per lane).
#[derive(Clone, Copy, Default)]
struct MidiCvLane {
    voice: VoiceState,
    /// Retrigger pulse gate (multi-sample duration via TempGate).
    retrigger_gate: TempGate,
}

/// Module-level state for the MidiCv module.
struct MidiCvState {
    sample_rate: f32,

    /// Held notes (order preserved for priority modes)
    held_notes: Vec<(u8, u8, u8)>, // (note, velocity, midi_channel)

    /// Current rotation index for voice allocation
    rotate_index: usize,

    /// Sustain pedal state per MIDI channel
    sustain: [bool; 16],

    /// Notes held by sustain (note, velocity, midi_channel)
    sustained_notes: Vec<(u8, u8, u8)>,

    /// Global pitch wheel (for non-MPE mode)
    global_pitch_wheel: i16,

    /// Global mod wheel (for non-MPE mode)
    global_mod_wheel: u8,

    /// Global aftertouch (for non-MPE mode)
    global_aftertouch: u8,
}

impl Default for MidiCvState {
    fn default() -> Self {
        Self {
            sample_rate: 48000.0,
            // Both note lists are deduped by note value on every push, so 256
            // (the full u8 note domain) bounds their length — the audio-thread
            // message handlers never grow them past this capacity.
            held_notes: Vec::with_capacity(256),
            rotate_index: 0,
            sustain: [false; 16],
            sustained_notes: Vec::with_capacity(256),
            global_pitch_wheel: 0,
            global_mod_wheel: 0,
            global_aftertouch: 0,
        }
    }
}

impl MidiCv {
    /// Convert MIDI note number to 1V/octave CV (0V = C4 = MIDI 60)
    fn note_to_cv(note: u8) -> f32 {
        (note as f32 - 60.0) / 12.0
    }

    /// Convert velocity (0-127) to voltage (0-5V)
    fn velocity_to_cv(velocity: u8) -> f32 {
        velocity as f32 / 127.0 * 5.0
    }

    /// Convert pitch bend to voltage offset based on range
    fn pitch_bend_to_cv(&self, pitch_bend: i16) -> f32 {
        if self.params.pitch_bend_range == 0 {
            return 0.0;
        }
        // pitch_bend is -8192 to 8191
        // Convert to semitones based on range, then to volts (1V/octave)
        let semitones = (pitch_bend as f32 / 8192.0) * self.params.pitch_bend_range as f32;
        semitones / 12.0
    }

    /// Check if we should process events from a MIDI device
    fn should_process_device(&self, device: Option<&DeviceName>) -> bool {
        match (&self.params.device, device) {
            (None, _) => true, // No filter = accept all devices
            (Some(wanted), Some(got)) => wanted == got.as_str(), // Exact match
            (Some(_), None) => false, // Filter set but no device info
        }
    }

    /// Check if we should process events from a MIDI channel
    fn should_process_channel(&self, midi_channel: u8) -> bool {
        match self.params.channel {
            None => true,                                       // Omni mode
            Some(ch) => midi_channel == (ch.saturating_sub(1)), // 1-indexed param to 0-indexed MIDI
        }
    }

    /// Find a free voice or steal one based on poly mode
    fn allocate_voice(&mut self, note: u8, midi_channel: u8) -> usize {
        let num_voices = self.params.lanes.clamp(1, PORT_MAX_LANES);

        match self.params.poly_mode {
            PolyMode::Mpe => {
                // In MPE mode, MIDI channel directly maps to voice
                // Channel 0 (master) typically maps to all voices, channels 1-15 map to voices 0-14
                if midi_channel == 0 {
                    0 // Master channel controls first voice
                } else {
                    ((midi_channel - 1) as usize).min(num_voices - 1)
                }
            }
            PolyMode::Reuse => {
                // First, try to find a voice already playing this note
                for i in 0..num_voices {
                    if self.lane_state[i].voice.gate && self.lane_state[i].voice.note == note {
                        return i;
                    }
                }
                // Fall through to rotate behavior
                self.allocate_voice_rotate(num_voices)
            }
            PolyMode::Reset => {
                // Always scan from 0, use first free voice
                for i in 0..num_voices {
                    if !self.lane_state[i].voice.gate {
                        return i;
                    }
                }
                // All busy: use last voice
                num_voices - 1
            }
            PolyMode::Rotate => self.allocate_voice_rotate(num_voices),
        }
    }

    fn allocate_voice_rotate(&mut self, num_voices: usize) -> usize {
        // Find next free voice starting from rotate_index
        for i in 0..num_voices {
            let idx = (self.state.rotate_index + i) % num_voices;
            if !self.lane_state[idx].voice.gate {
                self.state.rotate_index = (idx + 1) % num_voices;
                return idx;
            }
        }
        // All voices busy: steal from rotate_index
        let idx = self.state.rotate_index;
        self.state.rotate_index = (idx + 1) % num_voices;
        idx
    }

    /// Find which voice is playing a note
    fn find_voice_for_note(&self, note: u8) -> Option<usize> {
        let num_voices = self.params.lanes.clamp(1, PORT_MAX_LANES);
        (0..num_voices).find(|&i| {
            let voice = &self.lane_state[i].voice;
            voice.gate && voice.note == note
        })
    }

    /// Get the note to play in monophonic mode based on priority
    fn get_mono_note(&self) -> Option<(u8, u8)> {
        if self.state.held_notes.is_empty() {
            return None;
        }

        match self.params.mono_mode {
            MonoMode::Last => self.state.held_notes.last().map(|&(n, v, _)| (n, v)),
            MonoMode::First => self.state.held_notes.first().map(|&(n, v, _)| (n, v)),
            MonoMode::Lowest => self
                .state
                .held_notes
                .iter()
                .min_by_key(|&&(n, _, _)| n)
                .map(|&(n, v, _)| (n, v)),
            MonoMode::Highest => self
                .state
                .held_notes
                .iter()
                .max_by_key(|&&(n, _, _)| n)
                .map(|&(n, v, _)| (n, v)),
        }
    }

    /// Handle MIDI note on message
    fn on_midi_note_on(&mut self, msg: &MidiNoteOn) -> Result<()> {
        if !self.should_process_device(msg.device.as_ref())
            || !self.should_process_channel(msg.channel)
        {
            return Ok(());
        }

        let note = msg.note;
        let velocity = msg.velocity;
        let midi_channel = msg.channel;

        // Remove from held_notes if already present (shouldn't happen but be safe)
        self.state.held_notes.retain(|&(n, _, _)| n != note);
        self.state.held_notes.push((note, velocity, midi_channel));

        let num_voices = self.params.lanes.clamp(1, PORT_MAX_LANES);

        if num_voices == 1 {
            // Monophonic mode
            if let Some((mono_note, mono_vel)) = self.get_mono_note() {
                let voice = &mut self.lane_state[0].voice;
                let should_retrigger = !voice.gate || voice.note != mono_note;
                voice.note = mono_note;
                voice.velocity = mono_vel;
                voice.gate = true;
                if should_retrigger {
                    self.lane_state[0].retrigger_gate.set_state(
                        TempGateState::High,
                        TempGateState::Low,
                        min_gate_samples(self.state.sample_rate),
                    );
                }
            }
        } else {
            // Polyphonic mode
            let voice_idx = self.allocate_voice(note, midi_channel);
            let voice = &mut self.lane_state[voice_idx].voice;
            voice.note = note;
            voice.velocity = velocity;
            voice.gate = true;
            self.lane_state[voice_idx].retrigger_gate.set_state(
                TempGateState::High,
                TempGateState::Low,
                min_gate_samples(self.state.sample_rate),
            );
        }

        Ok(())
    }

    /// Handle MIDI note off message
    fn on_midi_note_off(&mut self, msg: &MidiNoteOff) -> Result<()> {
        if !self.should_process_device(msg.device.as_ref())
            || !self.should_process_channel(msg.channel)
        {
            return Ok(());
        }

        let note = msg.note;
        let midi_channel = msg.channel;

        // Check if sustain pedal is held for this MIDI channel
        let ch_idx = midi_channel as usize;
        if ch_idx < 16 && self.state.sustain[ch_idx] {
            // Move to sustained notes instead of releasing
            if let Some(idx) = self
                .state
                .held_notes
                .iter()
                .position(|&(n, _, _)| n == note)
            {
                let (n, v, c) = self.state.held_notes.remove(idx);
                // A re-struck note may already be parked here; keeping one
                // entry per note bounds the list to its pre-allocated
                // capacity, so the push never grows it on the audio thread.
                self.state.sustained_notes.retain(|&(sn, _, _)| sn != n);
                self.state.sustained_notes.push((n, v, c));
            }
            return Ok(());
        }

        // Remove from held notes
        self.state.held_notes.retain(|&(n, _, _)| n != note);
        self.state.sustained_notes.retain(|&(n, _, _)| n != note);

        let num_voices = self.params.lanes.clamp(1, PORT_MAX_LANES);

        if num_voices == 1 {
            // Monophonic: check if a different note should take over
            if let Some((mono_note, mono_vel)) = self.get_mono_note() {
                let voice = &mut self.lane_state[0].voice;
                if voice.note != mono_note {
                    voice.note = mono_note;
                    voice.velocity = mono_vel;
                    self.lane_state[0].retrigger_gate.set_state(
                        TempGateState::High,
                        TempGateState::Low,
                        min_gate_samples(self.state.sample_rate),
                    );
                }
            } else {
                self.lane_state[0].voice.gate = false;
            }
        } else {
            // Polyphonic: release the specific voice
            if let Some(voice_idx) = self.find_voice_for_note(note) {
                self.lane_state[voice_idx].voice.gate = false;
            }
        }

        Ok(())
    }

    /// Handle MIDI CC message (for sustain pedal and mod wheel)
    fn on_midi_cc(&mut self, msg: &MidiControlChange) -> Result<()> {
        if !self.should_process_device(msg.device.as_ref())
            || !self.should_process_channel(msg.channel)
        {
            return Ok(());
        }

        match msg.cc {
            1 => {
                // Mod wheel
                if self.params.poly_mode == PolyMode::Mpe && msg.channel > 0 {
                    let voice_idx =
                        ((msg.channel - 1) as usize).min(self.params.lanes.saturating_sub(1));
                    self.lane_state[voice_idx].voice.mod_wheel = msg.value;
                } else {
                    self.state.global_mod_wheel = msg.value;
                }
            }
            64 => {
                // Sustain pedal
                let held = msg.value >= 64;
                let ch_idx = msg.channel as usize;
                if ch_idx < 16 {
                    let was_held = self.state.sustain[ch_idx];
                    self.state.sustain[ch_idx] = held;

                    // When sustain is released, release all sustained notes for
                    // this channel — in place, so this message handler (which
                    // runs on the audio thread) never builds an intermediate
                    // list.
                    if was_held && !held {
                        let mut i = 0;
                        while i < self.state.sustained_notes.len() {
                            let (n, _, c) = self.state.sustained_notes[i];
                            if c == msg.channel {
                                if let Some(voice_idx) = self.find_voice_for_note(n) {
                                    self.lane_state[voice_idx].voice.gate = false;
                                }
                                self.state.sustained_notes.remove(i);
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle MIDI pitch bend message
    fn on_midi_pitch_bend(&mut self, msg: &MidiPitchBend) -> Result<()> {
        if !self.should_process_device(msg.device.as_ref())
            || !self.should_process_channel(msg.channel)
        {
            return Ok(());
        }

        if self.params.poly_mode == PolyMode::Mpe && msg.channel > 0 {
            // MPE: per-voice pitch bend
            let voice_idx = ((msg.channel - 1) as usize).min(self.params.lanes.saturating_sub(1));
            self.lane_state[voice_idx].voice.pitch_wheel = msg.value;
        } else {
            // Standard: global pitch bend
            self.state.global_pitch_wheel = msg.value;
        }

        Ok(())
    }

    /// Handle MIDI channel pressure message
    fn on_midi_channel_pressure(&mut self, msg: &MidiChannelPressure) -> Result<()> {
        if !self.should_process_device(msg.device.as_ref())
            || !self.should_process_channel(msg.channel)
        {
            return Ok(());
        }

        if self.params.poly_mode == PolyMode::Mpe && msg.channel > 0 {
            let voice_idx = ((msg.channel - 1) as usize).min(self.params.lanes.saturating_sub(1));
            self.lane_state[voice_idx].voice.aftertouch = msg.pressure;
        } else {
            self.state.global_aftertouch = msg.pressure;
        }

        Ok(())
    }

    /// Handle MIDI polyphonic pressure message
    fn on_midi_poly_pressure(&mut self, msg: &MidiPolyPressure) -> Result<()> {
        if !self.should_process_device(msg.device.as_ref())
            || !self.should_process_channel(msg.channel)
        {
            return Ok(());
        }

        // Find voice playing this note and update its aftertouch
        if let Some(voice_idx) = self.find_voice_for_note(msg.note) {
            self.lane_state[voice_idx].voice.aftertouch = msg.pressure;
        }

        Ok(())
    }

    /// Handle MIDI panic
    fn on_midi_panic(&mut self) -> Result<()> {
        self.state.held_notes.clear();
        self.state.sustained_notes.clear();
        self.state.rotate_index = 0;
        self.state.global_pitch_wheel = 0;
        self.state.global_mod_wheel = 0;
        self.state.global_aftertouch = 0;

        for i in 0..self.lane_state.len() {
            self.lane_state[i].voice = VoiceState::default();
            self.lane_state[i]
                .retrigger_gate
                .set_state(TempGateState::Low, TempGateState::Low, 0);
        }

        for i in 0..16 {
            self.state.sustain[i] = false;
        }

        Ok(())
    }

    fn update(&mut self, sample_rate: f32) {
        self.state.sample_rate = sample_rate;

        let num_voices = self.lane_count();

        // Update outputs for each voice
        for i in 0..num_voices {
            let voice = &self.lane_state[i].voice;

            // Pitch CV
            let pitch_cv = Self::note_to_cv(voice.note);
            let pitch_bend_cv = if self.params.poly_mode == PolyMode::Mpe {
                self.pitch_bend_to_cv(voice.pitch_wheel)
            } else {
                self.pitch_bend_to_cv(self.state.global_pitch_wheel)
            };
            self.outputs.pitch.set(i, pitch_cv + pitch_bend_cv);

            // Gate
            self.outputs.gate.set(
                i,
                if voice.gate {
                    GATE_HIGH_VOLTAGE
                } else {
                    GATE_LOW_VOLTAGE
                },
            );

            // Velocity
            self.outputs
                .velocity
                .set(i, Self::velocity_to_cv(voice.velocity));

            // Aftertouch
            let aftertouch = if self.params.poly_mode == PolyMode::Mpe {
                voice.aftertouch
            } else {
                self.state.global_aftertouch.max(voice.aftertouch)
            };
            self.outputs
                .aftertouch
                .set(i, aftertouch as f32 / 127.0 * 5.0);

            // Retrigger pulse
            self.outputs
                .retrigger
                .set(i, self.lane_state[i].retrigger_gate.process());

            // Pitch wheel (raw, unscaled, -5V to +5V)
            let pw = if self.params.poly_mode == PolyMode::Mpe {
                voice.pitch_wheel
            } else {
                self.state.global_pitch_wheel
            };
            self.outputs.pitch_wheel.set(i, pw as f32 / 8192.0 * 5.0);

            // Mod wheel
            let mw = if self.params.poly_mode == PolyMode::Mpe {
                voice.mod_wheel
            } else {
                self.state.global_mod_wheel
            };
            self.outputs.mod_wheel.set(i, mw as f32 / 127.0 * 5.0);
        }
    }
}

message_handlers!(impl MidiCv {
    MidiNoteOn(m) => MidiCv::on_midi_note_on,
    MidiNoteOff(m) => MidiCv::on_midi_note_off,
    MidiCC(m) => MidiCv::on_midi_cc,
    MidiPitchBend(m) => MidiCv::on_midi_pitch_bend,
    MidiChannelPressure(m) => MidiCv::on_midi_channel_pressure,
    MidiPolyPressure(m) => MidiCv::on_midi_poly_pressure,
    MidiPanic => MidiCv::on_midi_panic,
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_to_cv() {
        // C4 = MIDI 60 = 0V
        assert!((MidiCv::note_to_cv(60) - 0.0).abs() < 0.001);
        // C5 = MIDI 72 = 1V
        assert!((MidiCv::note_to_cv(72) - 1.0).abs() < 0.001);
        // C3 = MIDI 48 = -1V
        assert!((MidiCv::note_to_cv(48) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_velocity_to_cv() {
        assert!((MidiCv::velocity_to_cv(0) - 0.0).abs() < 0.001);
        assert!((MidiCv::velocity_to_cv(127) - 5.0).abs() < 0.001);
        assert!((MidiCv::velocity_to_cv(64) - 2.52).abs() < 0.1);
    }
}
