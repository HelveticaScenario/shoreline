//! The `voice` param of a `$mi.sixOp` playing one of Plaits' patches, as the
//! DSL sends it.

#[path = "../plaits_voices/mod.rs"]
mod plaits_voices;

use modular_core::dsp::mi::six_op::parse_syx;
use serde_json::{Value, json};

/// Plaits' patch `patch` of `bank`, from its builtin file.
pub fn plaits_voice(bank: usize, patch: usize) -> Value {
    let index = bank * 32 + patch;
    let voice = parse_syx(&plaits_voices::plaits_file(index))
        .unwrap()
        .remove(0);
    let path = plaits_voices::PLAITS_VOICES[index];
    json!({ "type": "dx7_voice", "path": format!("builtin/{path}"), "name": voice.name, "data": voice.data() })
}
