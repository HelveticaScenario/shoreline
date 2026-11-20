//! `$wavetable` renders identically on the block and per-sample paths: held and
//! modulated pitch and position, FM, phase offset, hard sync and phase warps,
//! band-limited or not.

mod block_common;

use block_common::*;
use modular_core::types::ProcessingMode;
use serde_json::json;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn wavetable_paths_agree() {
    let wav = json!({ "type": "wav_ref", "path": "table", "channels": 1 });
    check_paths_agree(
        "$wavetable",
        json!({ "wav": wav, "pitch": [0.0, 0.7, -1.3, 2.1], "position": 2.0 }),
    );
    check_paths_agree(
        "$wavetable",
        json!({
            "wav": wav,
            "pitch": poly_cable("lfo", LANES),
            "position": poly_cable("src", LANES),
            "fm": cable("lfo", 2),
            "phaseOffset": 0.3,
        }),
    );
    check_paths_agree(
        "$wavetable",
        json!({ "wav": wav, "pitch": 0.5, "sync": poly_cable("src", LANES) }),
    );
    check_paths_agree(
        "$wavetable",
        json!({
            "wav": wav,
            "pitch": 0.5,
            "sync": cable("src", 0),
            "phase": { "type": "mirror", "amount": cable("lfo", 1) },
        }),
    );
    check_paths_agree(
        "$wavetable",
        json!({
            "wav": wav,
            "pitch": poly_cable("lfo", LANES),
            "sync": cable("src", 0),
            "phase": { "type": "pulsar", "amount": 4.0 },
            "bandlimit": false,
        }),
    );
    check_paths_agree(
        "$wavetable",
        json!({ "wav": wav, "pitch": 3.0, "position": 5.0, "bandlimit": false }),
    );
}

/// Without band-limiting the full table plays at every pitch: below the
/// table's own frequency nothing changes, and a high note keeps the harmonics
/// the band-limited read drops.
#[test]
fn unbandlimited_reads_the_full_table() {
    let outputs = |pitch: f32, bandlimit: bool| {
        let wav = json!({ "type": "wav_ref", "path": "table", "channels": 1 });
        let patch = build(
            ProcessingMode::Block,
            ProcessingMode::Block,
            "$wavetable",
            json!({ "wav": wav, "pitch": pitch, "position": 5.0, "bandlimit": bandlimit }),
        );
        render(&patch, &[BLOCK_SIZE]).0
    };
    assert_eq!(outputs(-2.0, false), outputs(-2.0, true));
    assert_ne!(outputs(4.0, false), outputs(4.0, true));
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$wavetable",
            json!({ "wav": { "type": "wav_ref", "path": "table", "channels": 1 }, "pitch": poly_cable("lfo", LANES), "position": cable("src", 0), "sync": cable("src", 1) })
        ),
        0
    );
}
