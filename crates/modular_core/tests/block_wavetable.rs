//! `$wavetable` renders identically on the block and per-sample paths: held and
//! modulated pitch and position, FM, phase offset, hard sync, restart and phase
//! warps, from mono and two-channel tables.

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
        json!({ "wav": wav, "pitch": 0.5, "restart": poly_cable("src", LANES) }),
    );
    check_paths_agree(
        "$wavetable",
        json!({
            "wav": wav,
            "pitch": 0.5,
            "restart": cable("src", 0),
            "phase": { "type": "mirror", "amount": cable("lfo", 1) },
        }),
    );
    check_paths_agree(
        "$wavetable",
        json!({ "wav": wav, "pitch": [0.0, 1.5], "sync": 1.3 }),
    );
    check_paths_agree(
        "$wavetable",
        json!({ "wav": wav, "pitch": poly_cable("lfo", LANES), "sync": cable("lfo", 1), "restart": cable("src", 0), "phaseOffset": 0.25 }),
    );
}

fn stereo_wav() -> serde_json::Value {
    json!({ "type": "wav_ref", "path": "stereoTable", "channels": 2 })
}

/// Two lanes of a two-channel table, four physical lanes: every one is read.
#[test]
fn stereo_wavetable_paths_agree() {
    let wav = stereo_wav();
    check_paths_agree(
        "$wavetable",
        json!({ "wav": wav, "pitch": [0.0, 0.7], "position": 2.0 }),
    );
    check_paths_agree(
        "$wavetable",
        json!({
            "wav": wav,
            "pitch": poly_cable("lfo", 2),
            "position": poly_cable("src", 2),
            "fm": cable("lfo", 2),
            "phaseOffset": 0.3,
        }),
    );
    check_paths_agree(
        "$wavetable",
        json!({ "wav": wav, "pitch": [0.5, -0.4], "restart": cable("src", 1), "sync": cable("lfo", 1) }),
    );
    check_paths_agree(
        "$wavetable",
        json!({
            "wav": wav,
            "pitch": 0.5,
            "restart": cable("src", 0),
            "phase": { "type": "mirror", "amount": cable("lfo", 1) },
        }),
    );
}

/// Each side of a two-channel table plays exactly as a mono table of that
/// channel alone, so with mono inputs the sides share every phase, position
/// and reset.
#[test]
fn stereo_sides_play_each_channel_in_step() {
    let params = |wav: serde_json::Value| {
        json!({
            "wav": wav,
            "pitch": [cable("lfo", 0), cable("lfo", 3)],
            "position": [cable("src", 1), cable("src", 2)],
            "sync": cable("lfo", 1),
            "restart": cable("src", 0),
        })
    };
    let mono = |path: &str| json!({ "type": "wav_ref", "path": path, "channels": 1 });
    let modes = [
        (ProcessingMode::Block, ProcessingMode::Block),
        (ProcessingMode::Block, ProcessingMode::Sample),
    ];
    for (src_mode, dut_mode) in modes {
        let render_dut = |wav| {
            let patch = build(src_mode, dut_mode, "$wavetable", params(wav));
            render(&patch, &[BLOCK_SIZE]).0
        };
        let stereo = render_dut(stereo_wav());
        let (left, right) = (render_dut(mono("table")), render_dut(mono("rightTable")));
        // Stereo lane `l` side `c` is physical lane `l * 2 + c`; the mono
        // renders hold lanes 0 and 1 in physical lanes 0 and 1.
        for (slot, stereo) in stereo.chunks(LANES).enumerate() {
            let at = |render: &[f32], lane: usize| render[slot * LANES + lane];
            for lane in 0..2 {
                assert_eq!(
                    stereo[lane * 2],
                    at(&left, lane),
                    "{dut_mode:?} slot {slot} lane {lane} left"
                );
                assert_eq!(
                    stereo[lane * 2 + 1],
                    at(&right, lane),
                    "{dut_mode:?} slot {slot} lane {lane} right"
                );
            }
        }
        assert_ne!(left, right);
    }
}

#[test]
fn allocation_free() {
    assert_eq!(
        allocations_while_rendering(
            "$wavetable",
            json!({ "wav": { "type": "wav_ref", "path": "table", "channels": 1 }, "pitch": poly_cable("lfo", LANES), "position": cable("src", 0), "restart": cable("src", 1), "sync": cable("lfo", 1) })
        ),
        0
    );
    assert_eq!(
        allocations_while_rendering(
            "$wavetable",
            json!({ "wav": stereo_wav(), "pitch": poly_cable("lfo", 2), "position": cable("src", 0), "restart": cable("src", 1), "sync": cable("lfo", 1) })
        ),
        0
    );
}
