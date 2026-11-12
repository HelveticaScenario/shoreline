//! Engine bounds: every read of a module's bounds belongs to the sample read at
//! the same slot, on every path a value takes, and a patch settles which
//! outputs carry bounds, and which store them, before it runs.

use modular_core::Patch;
use modular_core::bounds::Bounds;
use modular_core::dsp::get_params_deserializers;
use modular_core::params::DeserializedParams;
use modular_core::types::{ProcessingMode, SameIds};
use serde_json::{Value, json};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 64;
const BLOCKS: usize = 4;

fn cable(module: &str) -> Value {
    json!({ "type": "cable", "module": module, "port": "output", "channel": 0 })
}

/// `modules`, each read by a `$remap` that maps it from its own range, so
/// every bounded output stores its bounds.
fn build(modules: &[(&str, &str, Value, ProcessingMode)]) -> Patch {
    build_unread(&with_readers(modules))
}

fn with_readers(
    modules: &[(&str, &str, Value, ProcessingMode)],
) -> Vec<(String, &'static str, Value, ProcessingMode)> {
    let readers = modules.iter().map(|(id, ..)| {
        (
            format!("read_{id}"),
            "$remap",
            json!({ "input": cable(id), "outMin": 0.0, "outMax": 1.0 }),
            ProcessingMode::Block,
        )
    });
    modules
        .iter()
        .map(|(id, module_type, params, mode)| {
            let module_type: &'static str = Box::leak(module_type.to_string().into_boxed_str());
            (id.to_string(), module_type, params.clone(), *mode)
        })
        .chain(readers)
        .collect()
}

fn build_unread(modules: &[(String, &str, Value, ProcessingMode)]) -> Patch {
    let deserializers = get_params_deserializers();
    let mut patch = Patch::new();
    patch
        .insert_modules(
            modules.iter().map(|(id, module_type, params, mode)| {
                let cached = deserializers[*module_type](params.clone()).unwrap();
                (
                    id.to_string(),
                    module_type.to_string(),
                    DeserializedParams {
                        params: cached.params,
                        channel_count: cached.channel_count,
                        f64_inputs: false,
                    },
                    *mode,
                )
            }),
            SAMPLE_RATE,
            BLOCK_SIZE,
        )
        .unwrap();
    patch.connect_all();
    patch
}

/// Render one block, advancing every module through `steps`, `order` first.
fn render_block(patch: &Patch, order: &[&str], steps: &[usize]) {
    for module in patch.sampleables.values() {
        module.start_block();
    }
    for &step in steps {
        for id in order {
            patch.sampleables[*id].ensure_processed_to(step);
        }
    }
}

fn read(patch: &Patch, id: &str, slot: usize) -> (f32, Bounds) {
    let module = &patch.sampleables[id];
    (
        module.get_value_at("output", 0, slot),
        module.get_bounds_at("output", 0, slot),
    )
}

/// A source whose value and bounds both move every sample: a saw remapped
/// onto a range whose top an LFO sweeps.
fn moving_source(mode: ProcessingMode) -> [(&'static str, &'static str, Value, ProcessingMode); 3] {
    [
        ("saw", "$saw", json!({ "freq": 2.0 }), ProcessingMode::Block),
        (
            "lfo",
            "$sine",
            json!({ "freq": -1.0 }),
            ProcessingMode::Block,
        ),
        (
            "src",
            "$remap",
            json!({
                "input": cable("saw"),
                "outMin": -1.0,
                "outMax": cable("lfo"),
                "inMin": -5.0,
                "inMax": 5.0,
            }),
            mode,
        ),
    ]
}

/// `dut` reads `src` through `$add` with an offset of 0, so its value and
/// bounds equal the source's at the slot it read.
fn assert_lock_step(patch: &Patch, order: &[&str], steps: &[usize]) {
    for _ in 0..BLOCKS {
        render_block(patch, order, steps);
        for slot in 0..BLOCK_SIZE {
            let (value, bounds) = read(patch, "dut", slot);
            let (src_value, src_bounds) = read(patch, "src", slot);
            assert_eq!(value, src_value, "slot {slot}");
            assert_eq!(bounds, src_bounds, "slot {slot}");
            assert!(bounds.is_bounded());
        }
    }
}

#[test]
fn bounds_stay_in_lock_step_on_every_read_path() {
    use ProcessingMode::{Block, Sample};
    let step_patterns: [&[usize]; 2] = [&[BLOCK_SIZE], &[1, 10, 11, 40, BLOCK_SIZE]];
    for (src_mode, dut_mode) in [
        (Block, Block),
        (Block, Sample),
        (Sample, Block),
        (Sample, Sample),
    ] {
        for steps in step_patterns {
            for order in [["dut", "src", "saw", "lfo"], ["saw", "lfo", "src", "dut"]] {
                let mut modules = moving_source(src_mode).to_vec();
                modules.push((
                    "dut",
                    "$add",
                    json!({ "input": cable("src"), "offset": 0.0 }),
                    dut_mode,
                ));
                assert_lock_step(&build(&modules), &order, steps);
            }
        }
    }
}

#[test]
fn bounds_stay_in_lock_step_across_a_patch_swap() {
    let modules = |offset: f32| {
        let mut modules = moving_source(ProcessingMode::Block).to_vec();
        modules.push((
            "dut",
            "$add",
            json!({ "input": cable("src"), "offset": offset }),
            ProcessingMode::Block,
        ));
        modules
    };
    let order = ["dut", "src", "saw", "lfo"];
    let old = build(&modules(0.0));
    for _ in 0..BLOCKS {
        render_block(&old, &order, &[BLOCK_SIZE]);
    }
    let new = build(&modules(0.0));
    for (id, module) in &new.sampleables {
        module.transfer_state_from(old.sampleables[id].as_ref(), &SameIds);
    }
    new.connect_all();
    assert_lock_step(&new, &order, &[1, 10, 11, 40, BLOCK_SIZE]);
}

#[test]
fn a_feedback_cycle_reads_the_delayed_slots_bounds() {
    // `a` remaps `b`, and `b` adds 0.5 to `a`: one of them reads the other a
    // sample late, and the bounds it reads are those of that same sample.
    let modules = [
        ("saw", "$saw", json!({ "freq": 2.0 }), ProcessingMode::Block),
        (
            "lfo",
            "$sine",
            json!({ "freq": -1.0 }),
            ProcessingMode::Block,
        ),
        (
            "a",
            "$remap",
            json!({
                "input": cable("b"),
                "outMin": cable("saw"),
                "outMax": cable("lfo"),
                "inMin": -5.0,
                "inMax": 5.0,
            }),
            ProcessingMode::Sample,
        ),
        (
            "b",
            "$add",
            json!({ "input": cable("a"), "offset": 0.5 }),
            ProcessingMode::Sample,
        ),
    ];
    let patch = build(&modules);
    let shift = |b: Bounds| b.map(|x| x + 0.5);
    let mut previous = None;
    for _ in 0..BLOCKS {
        render_block(&patch, &["b", "a", "saw", "lfo"], &[BLOCK_SIZE]);
        for slot in 0..BLOCK_SIZE {
            let (value, bounds) = read(&patch, "b", slot);
            assert!(bounds.is_bounded());
            let now = read(&patch, "a", slot);
            let read_from = if value == now.0 + 0.5 {
                Some(now)
            } else {
                previous
            };
            let (a_value, a_bounds) = read_from.expect("b reads a sample of a");
            assert_eq!(value, a_value + 0.5, "slot {slot}");
            assert_eq!(bounds, shift(a_bounds), "slot {slot}");
            previous = Some(now);
        }
    }
}

#[test]
fn a_cycle_that_only_feeds_itself_stays_unbounded() {
    let modules = [
        (
            "a",
            "$add",
            json!({ "input": cable("b"), "offset": 1.0 }),
            ProcessingMode::Sample,
        ),
        (
            "b",
            "$amp",
            json!({ "input": cable("a"), "factor": 2.5 }),
            ProcessingMode::Sample,
        ),
        (
            "c",
            "$add",
            json!({ "input": cable("a"), "offset": 0.0 }),
            ProcessingMode::Block,
        ),
    ];
    let patch = build(&modules);
    render_block(&patch, &["c", "a", "b"], &[BLOCK_SIZE]);
    for id in ["a", "b", "c"] {
        assert_eq!(read(&patch, id, 10).1, Bounds::UNBOUNDED, "{id}");
    }
}

#[test]
fn a_hold_keeps_the_bounds_its_sample_was_taken_under() {
    for (module, gate) in [("$sah", "trigger"), ("$tah", "gate")] {
        let mut modules = moving_source(ProcessingMode::Block).to_vec();
        modules.push((
            "clock",
            "$pulse",
            json!({ "freq": 4.0 }),
            ProcessingMode::Block,
        ));
        modules.push((
            "dut",
            module,
            json!({ "input": cable("src"), gate: cable("clock") }),
            ProcessingMode::Block,
        ));
        let patch = build(&modules);
        // A hold that has not yet taken a sample holds 0 V.
        let mut history = vec![(0.0, Bounds::point(0.0))];
        for _ in 0..BLOCKS {
            render_block(
                &patch,
                &["dut", "src", "saw", "lfo", "clock"],
                &[BLOCK_SIZE],
            );
            for slot in 0..BLOCK_SIZE {
                history.push(read(&patch, "src", slot));
                let held = read(&patch, "dut", slot);
                assert!(held.1.is_bounded(), "{module} slot {slot}: {held:?}");
                assert!(history.contains(&held), "{module} slot {slot}: {held:?}");
            }
        }
    }
}

#[test]
fn slew_and_wrap_bounds() {
    let modules = [
        ("saw", "$saw", json!({ "freq": 2.0 }), ProcessingMode::Block),
        (
            "slew",
            "$slew",
            json!({ "input": cable("saw") }),
            ProcessingMode::Block,
        ),
        (
            "wrap",
            "$wrap",
            json!({ "input": cable("saw"), "min": 1.0, "max": -2.0 }),
            ProcessingMode::Block,
        ),
        (
            "octave",
            "$wrap",
            json!({ "input": cable("saw"), "min": 1.0, "max": -2.0, "octave": true }),
            ProcessingMode::Block,
        ),
    ];
    let patch = build(&modules);
    render_block(&patch, &["slew", "wrap", "octave", "saw"], &[BLOCK_SIZE]);
    assert_eq!(read(&patch, "slew", 3).1, Bounds::new(-5.0, 5.0));
    assert_eq!(read(&patch, "wrap", 3).1, Bounds::new(-2.0, 1.0));
    assert_eq!(read(&patch, "octave", 3).1, Bounds::new(-2.5, 1.5));
}

#[test]
fn an_input_from_an_unbounded_output_leaves_a_module_unbounded() {
    let modules = [
        (
            "noise",
            "$noise",
            json!({ "color": "white" }),
            ProcessingMode::Block,
        ),
        (
            "dut",
            "$add",
            json!({ "input": cable("noise"), "offset": 1.0 }),
            ProcessingMode::Block,
        ),
        (
            "fixed",
            "$add",
            json!({ "input": 2.0, "offset": 1.0 }),
            ProcessingMode::Block,
        ),
    ];
    let patch = build(&modules);
    render_block(&patch, &["dut", "fixed", "noise"], &[BLOCK_SIZE]);
    let noise_bounded = read(&patch, "noise", 0).1.is_bounded();
    assert_eq!(read(&patch, "dut", 0).1.is_bounded(), noise_bounded);
    assert_eq!(read(&patch, "fixed", 0).1, Bounds::point(3.0));
}

/// A source whose bounds hold for stretches and step mid-block: a saw
/// remapped onto a range whose top a pulse switches.
fn stepping_source(
    mode: ProcessingMode,
) -> [(&'static str, &'static str, Value, ProcessingMode); 3] {
    [
        ("saw", "$saw", json!({ "freq": 2.0 }), ProcessingMode::Block),
        (
            "lfo",
            "$pulse",
            json!({ "freq": 3.0 }),
            ProcessingMode::Block,
        ),
        (
            "src",
            "$remap",
            json!({
                "input": cable("saw"),
                "outMin": -1.0,
                "outMax": cable("lfo"),
                "inMin": -5.0,
                "inMax": 5.0,
            }),
            mode,
        ),
    ]
}

#[test]
fn bounds_that_step_mid_block_stay_in_lock_step() {
    use ProcessingMode::{Block, Sample};
    let step_patterns: [&[usize]; 3] = [
        &[BLOCK_SIZE],
        &[1, 10, 11, 40, BLOCK_SIZE],
        &[33, BLOCK_SIZE],
    ];
    for (src_mode, dut_mode) in [
        (Block, Block),
        (Block, Sample),
        (Sample, Block),
        (Sample, Sample),
    ] {
        for steps in step_patterns {
            for order in [["dut", "src", "saw", "lfo"], ["saw", "lfo", "src", "dut"]] {
                let mut modules = stepping_source(src_mode).to_vec();
                modules.push((
                    "dut",
                    "$add",
                    json!({ "input": cable("src"), "offset": 0.0 }),
                    dut_mode,
                ));
                let patch = build(&modules);
                let mut stepped = false;
                for _ in 0..BLOCKS {
                    render_block(&patch, &order, steps);
                    for slot in 0..BLOCK_SIZE {
                        let (value, bounds) = read(&patch, "dut", slot);
                        let source = read(&patch, "src", slot);
                        assert_eq!((value, bounds), source, "slot {slot}");
                        stepped |= slot > 0 && bounds != read(&patch, "src", slot - 1).1;
                    }
                }
                assert!(stepped, "the bounds step within a block");
            }
        }
    }
}

#[test]
fn a_cycle_with_steady_bounds_reads_them_across_blocks() {
    // The bounds hold all block, so they are kept as one row; slot 0 of each
    // block reads the previous block's last slot back through the delay.
    let modules = [
        (
            "a",
            "$remap",
            json!({
                "input": cable("b"),
                "outMin": -1.0,
                "outMax": 2.0,
                "inMin": -5.0,
                "inMax": 5.0,
            }),
            ProcessingMode::Sample,
        ),
        (
            "b",
            "$add",
            json!({ "input": cable("a"), "offset": 0.5 }),
            ProcessingMode::Sample,
        ),
    ];
    let patch = build(&modules);
    for block in 0..BLOCKS {
        render_block(&patch, &["a", "b"], &[BLOCK_SIZE]);
        for slot in 0..BLOCK_SIZE {
            assert_eq!(
                read(&patch, "a", slot).1,
                Bounds::new(-1.0, 2.0),
                "block {block} slot {slot}"
            );
            // Before the first block, the delayed read finds 0 V and its
            // bounds; from then on, the previous block's last slot.
            if block > 0 || slot > 0 {
                assert_eq!(
                    read(&patch, "b", slot).1,
                    Bounds::new(-0.5, 2.5),
                    "block {block} slot {slot}"
                );
            }
        }
    }
}

#[test]
fn bounds_stay_in_lock_step_across_a_mid_block_swap() {
    let modules = |offset: f32| {
        let mut modules = stepping_source(ProcessingMode::Block).to_vec();
        modules.push((
            "dut",
            "$add",
            json!({ "input": cable("src"), "offset": offset }),
            ProcessingMode::Block,
        ));
        modules
    };
    let order = ["dut", "src", "saw", "lfo"];
    let old = build(&modules(0.0));
    render_block(&old, &order, &[BLOCK_SIZE]);
    for module in old.sampleables.values() {
        module.start_block();
    }
    let swap_at = 20;
    for id in order {
        old.sampleables[id].ensure_processed_to(swap_at);
    }
    let new = build(&modules(0.0));
    for (id, module) in &new.sampleables {
        if let Some(previous) = old.sampleables.get(id) {
            module.transfer_state_from(previous.as_ref(), &SameIds);
        }
    }
    new.connect_all();
    for module in new.sampleables.values() {
        module.set_initial_index(swap_at);
    }
    for id in order {
        new.sampleables[id].ensure_processed_to(BLOCK_SIZE);
    }
    for slot in swap_at..BLOCK_SIZE {
        assert_eq!(
            read(&new, "dut", slot),
            read(&new, "src", slot),
            "slot {slot}"
        );
    }
    assert_lock_step(&new, &order, &[1, 10, 11, 40, BLOCK_SIZE]);
}

#[test]
fn only_outputs_whose_bounds_are_read_store_them() {
    let modules = [
        ("saw", "$saw", json!({ "freq": 2.0 }), ProcessingMode::Block),
        (
            "unread",
            "$add",
            json!({ "input": cable("saw"), "offset": 1.0 }),
            ProcessingMode::Block,
        ),
        (
            "read",
            "$add",
            json!({ "input": cable("saw"), "offset": 2.0 }),
            ProcessingMode::Block,
        ),
        (
            "reader",
            "$remap",
            json!({ "input": cable("read"), "outMin": 0.0, "outMax": 1.0 }),
            ProcessingMode::Block,
        ),
    ];
    let modules: Vec<_> = modules
        .into_iter()
        .map(|(id, module_type, params, mode)| (id.to_string(), module_type, params, mode))
        .collect();
    let patch = build_unread(&modules);
    render_block(&patch, &["reader", "unread", "read", "saw"], &[BLOCK_SIZE]);
    assert_eq!(read(&patch, "read", 3).1, Bounds::new(-3.0, 7.0));
    assert_eq!(read(&patch, "unread", 3).1, Bounds::UNBOUNDED);
    assert_eq!(patch.bounds.read.get("unread"), None);
    // The reader maps from -3..7 onto 0..1: the saw at -5 reads 0.
    let (saw, _) = read(&patch, "saw", 3);
    let (mapped, _) = read(&patch, "reader", 3);
    assert!(
        (mapped - (saw + 2.0 + 3.0) / 10.0).abs() < 1e-6,
        "{mapped} from {saw}"
    );
}

#[test]
fn a_declared_range_bounds_a_signal_whose_value_has_none() {
    let modules = [
        ("saw", "$saw", json!({ "freq": 2.0 }), ProcessingMode::Block),
        (
            "filter",
            "$lpf",
            json!({ "input": cable("saw"), "cutoff": 0.0 }),
            ProcessingMode::Block,
        ),
        (
            "unranged",
            "$signal",
            json!({ "value": cable("filter"), "min": 0.0, "max": 10.0 }),
            ProcessingMode::Block,
        ),
        (
            "constant",
            "$signal",
            json!({ "value": 3.0, "min": 0.0, "max": 10.0 }),
            ProcessingMode::Block,
        ),
        (
            "swinging",
            "$signal",
            json!({ "value": cable("saw"), "min": 0.0, "max": 10.0 }),
            ProcessingMode::Block,
        ),
        (
            "control",
            "_smoothControl",
            json!({ "value": 3.0, "min": 0.0, "max": 10.0 }),
            ProcessingMode::Block,
        ),
    ];
    let patch = build(&modules);
    let order = [
        "unranged", "constant", "swinging", "control", "filter", "saw",
    ];
    render_block(&patch, &order, &[BLOCK_SIZE]);
    assert!(!read(&patch, "filter", 0).1.is_bounded());
    let declared = Bounds::new(0.0, 10.0);
    for slot in 0..BLOCK_SIZE {
        assert_eq!(read(&patch, "unranged", slot).1, declared, "slot {slot}");
        assert_eq!(read(&patch, "constant", slot).1, declared, "slot {slot}");
        assert_eq!(read(&patch, "control", slot).1, declared, "slot {slot}");
        assert_eq!(
            read(&patch, "swinging", slot).1,
            read(&patch, "saw", slot).1,
            "a value that swings keeps its own range, slot {slot}"
        );
    }
}

#[test]
fn a_declared_range_needs_both_ends_apart() {
    let deserializers = get_params_deserializers();
    for module_type in ["$signal", "_smoothControl"] {
        for params in [
            json!({ "value": 1.0, "min": 0.0 }),
            json!({ "value": 1.0, "max": 0.0 }),
            json!({ "value": 1.0, "min": 2.0, "max": 2.0 }),
        ] {
            assert!(
                deserializers[module_type](params.clone()).is_err(),
                "{module_type} {params}"
            );
        }
        assert!(
            deserializers[module_type](json!({ "value": 1.0, "min": 5.0, "max": 0.0 })).is_ok()
        );
    }
}

#[test]
fn a_control_without_a_range_holds_its_value_as_a_point() {
    let modules = [(
        "control",
        "_smoothControl",
        json!({ "value": 2.0 }),
        ProcessingMode::Block,
    )];
    let patch = build(&modules);
    render_block(&patch, &["control"], &[BLOCK_SIZE]);
    for slot in 0..BLOCK_SIZE {
        assert_eq!(read(&patch, "control", slot).1, Bounds::point(2.0));
    }
}

#[test]
fn rounding_rounds_the_range_in_its_order() {
    let source = |id: &'static str, out_min: f32, out_max: f32| {
        (
            id,
            "$remap",
            json!({
                "input": cable("saw"),
                "outMin": out_min,
                "outMax": out_max,
                "inMin": -5.0,
                "inMax": 5.0,
            }),
            ProcessingMode::Block,
        )
    };
    let rounding = |id: &'static str, module: &'static str, input: &str| {
        (
            id,
            module,
            json!({ "input": cable(input) }),
            ProcessingMode::Block,
        )
    };
    let modules = [
        ("saw", "$saw", json!({ "freq": 2.0 }), ProcessingMode::Block),
        source("up", -1.5, 3.5),
        source("down", 3.5, -1.5),
        rounding("round", "$round", "up"),
        rounding("floor", "$floor", "up"),
        rounding("ceil", "$ceil", "up"),
        rounding("round_down", "$round", "down"),
        rounding("floor_down", "$floor", "down"),
        rounding("ceil_down", "$ceil", "down"),
    ];
    let patch = build(&modules);
    let order: Vec<&str> = modules.iter().map(|(id, ..)| *id).collect();
    render_block(&patch, &order, &[BLOCK_SIZE]);
    for (id, expected) in [
        ("round", Bounds::new(-2.0, 4.0)),
        ("floor", Bounds::new(-2.0, 3.0)),
        ("ceil", Bounds::new(-1.0, 4.0)),
        ("round_down", Bounds::new(4.0, -2.0)),
        ("floor_down", Bounds::new(3.0, -2.0)),
        ("ceil_down", Bounds::new(4.0, -1.0)),
    ] {
        for slot in 0..BLOCK_SIZE {
            assert_eq!(read(&patch, id, slot).1, expected, "{id} slot {slot}");
        }
    }
}
