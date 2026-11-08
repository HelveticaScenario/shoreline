//! MIDI CC (Control Change) to CV converter module.
//!
//! Converts MIDI CC messages to control voltage signals.
//! Supports both 7-bit (standard) and 14-bit (high-resolution) CC.

use deserr::Deserr;
use napi::Result;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::types::{DeviceName, MidiControlChange, MidiControlChange14Bit};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct MidiCcParams {
    #[input("MIDI device to listen to; every device when unset")]
    #[deserr(default)]
    device: Option<String>,

    #[input("controller number to follow; 0–31 in high-resolution mode", range = (0, 127))]
    cc: u8,

    #[input("MIDI channel to listen to; every channel when unset", range = (1, 16))]
    #[deserr(default)]
    channel: Option<u8>,

    #[input("glide time between controller values, in milliseconds; 0 jumps instantly", range = (0.0, ..))]
    #[serde(default)]
    #[deserr(default)]
    smoothing_ms: f32,

    #[input("read 14-bit controllers, pairing CC 0–31 with CC 32–63 for finer steps")]
    #[serde(default)]
    #[deserr(default)]
    high_resolution: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MidiCcOutputs {
    #[output("output", "the controller's value: 0 at its minimum, 5 at its maximum", default, range = (0.0, 5.0))]
    output: f32,
}

/// State for the MidiCc module.
#[derive(Default)]
struct MidiCcState {
    /// Current CC value (normalized 0.0-1.0, supports both 7-bit and 14-bit)
    current_value: f32,
    /// Smoothed output value
    smoothed_value: f32,
}

const MIDI_CC_DOCS: ModuleDocs = ModuleDocs {
    summary: "MIDI controller to voltage",
    description: "Follows one knob, fader or wheel on a MIDI controller as a 0–5 V control \
                  voltage, for playing any param of a patch by hand.",
    categories: &[ModuleCategory::Midi],
    sections: &[],
    examples: &[DocExample {
        title: "Mod wheel filter sweep",
        code: "const wheel = $midiCC({ cc: 1, smoothingMs: 10 })\n\
               $saw('c2').$.lpf(wheel.range('c3', 'c7'), 2).out()",
    }],
};

#[module(name = "$midiCC", docs = MIDI_CC_DOCS, args())]
pub struct MidiCc {
    outputs: MidiCcOutputs,
    params: MidiCcParams,
    state: MidiCcState,
}

impl MidiCc {
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

    /// Handle 7-bit MIDI CC message
    fn on_midi_cc(&mut self, msg: &MidiControlChange) -> Result<()> {
        // Skip 7-bit messages if in high-resolution mode (we'll use 14-bit instead)
        if self.params.high_resolution {
            return Ok(());
        }

        if msg.cc == self.params.cc
            && self.should_process_device(msg.device.as_ref())
            && self.should_process_channel(msg.channel)
        {
            // Normalize 7-bit value (0-127) to 0.0-1.0
            self.state.current_value = msg.value as f32 / 127.0;
        }
        Ok(())
    }

    /// Handle 14-bit MIDI CC message
    fn on_midi_cc_14bit(&mut self, msg: &MidiControlChange14Bit) -> Result<()> {
        // Only process if in high-resolution mode
        if !self.params.high_resolution {
            return Ok(());
        }

        if msg.cc == self.params.cc
            && self.should_process_device(msg.device.as_ref())
            && self.should_process_channel(msg.channel)
        {
            // Normalize 14-bit value (0-16383) to 0.0-1.0
            // Note: max useful value is 127*128=16256 (MSB=127, LSB=0)
            // but we normalize to full 14-bit range for simplicity
            self.state.current_value = msg.value as f32 / 16383.0;
        }
        Ok(())
    }

    fn update(&mut self, sample_rate: f32) {
        // Calculate target voltage from normalized value
        let target = self.state.current_value * 5.0;

        // Apply smoothing
        if self.params.smoothing_ms > 0.0 {
            let smoothing_samples = self.params.smoothing_ms * sample_rate / 1000.0;
            let alpha = 1.0 / smoothing_samples.max(1.0);
            self.state.smoothed_value += (target - self.state.smoothed_value) * alpha;
        } else {
            self.state.smoothed_value = target;
        }

        self.outputs.output = self.state.smoothed_value;
    }
}

message_handlers!(impl MidiCc {
    MidiCC(m) => MidiCc::on_midi_cc,
    MidiCC14Bit(m) => MidiCc::on_midi_cc_14bit,
});
