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

/// `dut` rendered with `before` params, then swapped to `after` as the audio
/// thread swaps patches and rendered on; returns what it renders after the
/// swap.
fn render_swapped(
    dut_mode: ProcessingMode,
    before: serde_json::Value,
    after: serde_json::Value,
) -> Vec<f32> {
    let old = build(ProcessingMode::Block, dut_mode, "$wavetable", before);
    render(&old, &[BLOCK_SIZE]);
    let new = build(ProcessingMode::Block, dut_mode, "$wavetable", after);
    for (id, module) in &new.sampleables {
        module.transfer_state_from(old.sampleables[id].as_ref(), &modular_core::types::SameIds);
    }
    new.connect_all();
    render(&new, &[BLOCK_SIZE]).0
}

/// Swapped from a mono table to a two-channel one, both sides carry on from
/// the mono lane's phase, as a mono table of each channel would.
#[test]
fn a_table_widened_to_stereo_keeps_its_phase_on_both_sides() {
    let params = |wav: serde_json::Value| {
        json!({
            "wav": wav,
            "pitch": [cable("lfo", 0), cable("lfo", 3)],
            "position": cable("src", 1),
            "restart": cable("src", 0),
        })
    };
    let mono = |path: &str| json!({ "type": "wav_ref", "path": path, "channels": 1 });
    for dut_mode in [ProcessingMode::Block, ProcessingMode::Sample] {
        let swap = |after| render_swapped(dut_mode, params(mono("table")), params(after));
        let stereo = swap(stereo_wav());
        let (left, right) = (swap(mono("table")), swap(mono("rightTable")));
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
    }
    check_swap_agrees("$wavetable", params(mono("table")), params(stereo_wav()));
}

/// A stereo input runs each side on its own side of the input, so the sides
/// of a two-channel table part ways.
#[test]
fn a_stereo_pitch_plays_each_side_at_its_own_pitch() {
    let render_dut = |modules: Vec<(&str, &str, serde_json::Value)>| {
        let patch = assemble(
            modules
                .into_iter()
                .map(|(id, module_type, params)| (id, module_type, params, ProcessingMode::Block)),
            [("stereoTable", stereo_wavetable())],
        );
        let dut = &patch.sampleables["dut"];
        let mut out = Vec::new();
        for _ in 0..BLOCKS {
            for module in patch.sampleables.values() {
                module.start_block();
            }
            for module in patch.sampleables.values() {
                module.ensure_processed_to(BLOCK_SIZE);
            }
            for i in 0..BLOCK_SIZE {
                out.push([0, 1].map(|lane| dut.get_value_at("output", lane, i)));
            }
        }
        out
    };
    let at = |pitch: f32| {
        render_dut(vec![(
            "dut",
            "$wavetable",
            json!({ "wav": stereo_wav(), "pitch": pitch }),
        )])
    };
    let lane =
        |module: &str| json!({ "type": "cable", "module": module, "port": "output", "lane": 0 });
    let split = render_dut(vec![
        ("l", "$signal", json!({ "value": 0.0 })),
        ("r", "$signal", json!({ "value": 0.5 })),
        (
            "st",
            "$stereo",
            json!({ "left": lane("l"), "right": lane("r") }),
        ),
        (
            "dut",
            "$wavetable",
            json!({ "wav": stereo_wav(), "pitch": lane("st") }),
        ),
    ]);
    let (low, high) = (at(0.0), at(0.5));
    for (i, sides) in split.iter().enumerate() {
        assert_eq!(sides[0], low[i][0], "slot {i} left");
        assert_eq!(sides[1], high[i][1], "slot {i} right");
    }
    assert_ne!(low, high);
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
