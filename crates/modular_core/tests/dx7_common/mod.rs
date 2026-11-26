//! The `voice` param of a `$mi.sixOp` playing one of Plaits' patches, as the
//! DSL sends it.

#[path = "../plaits_voices/mod.rs"]
mod plaits_voices;

use modular_core::dsp::mi::six_op::parse_syx;
use serde_json::{Value, json};

/// Plaits' patch `patch` of `bank`, from its builtin file.
pub fn plaits_voice(bank: usize, patch: usize) -> Value {
    let path = plaits_voices::PLAITS_VOICES[bank * 32 + patch];
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/builtin")
        .join(path);
    let voice = parse_syx(&std::fs::read(file).unwrap()).unwrap().remove(0);
    let data: String = voice.packed.iter().map(|b| format!("{b:02x}")).collect();
    json!({ "type": "dx7_voice", "path": format!("builtin/{path}"), "name": voice.name, "data": data })
}
