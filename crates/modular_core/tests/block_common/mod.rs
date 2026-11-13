//! Shared harness for `#[module(block)]` tests. A block module must produce the
//! same samples whichever way its inputs resolve — whole-block rows, inputs
//! copied out slot by slot, or the per-sample `update` in a feedback cycle —
//! and must not touch the heap on any of those paths.
//!
//! Each block module's test file uses the parts it needs, so items unused by a
//! given test binary are expected.
#![allow(dead_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use modular_core::Patch;
pub use modular_core::bounds::Bounds;
use modular_core::dsp::{get_params_deserializers, schema};
use modular_core::params::DeserializedParams;
use modular_core::types::{ProcessingMode, SameIds, SampleBuffer, WavData};
use serde_json::{Value, json};

pub const SAMPLE_RATE: f32 = 48000.0;
pub const BLOCK_SIZE: usize = 64;
pub const LANES: usize = 4;
pub const BLOCKS: usize = 4;

pub fn cable(module: &str, lane: usize) -> Value {
    json!({ "type": "cable", "module": module, "port": "output", "lane": lane })
}

/// The output `module_type` gives when read as a signal itself.
pub fn default_port(module_type: &str) -> String {
    let outputs = schema()
        .into_iter()
        .find(|s| s.name == module_type)
        .unwrap()
        .outputs;
    let default = outputs.iter().position(|o| o.default).unwrap_or(0);
    outputs[default].name.clone()
}

pub fn poly_cable(module: &str, lanes: usize) -> Value {
    Value::Array((0..lanes).map(|lane| cable(module, lane)).collect())
}

/// `src` (a `LANES`-wide saw) and `lfo` (a `LANES`-wide sine) feed
/// `dut`, a `module_type` with `params`, and `reader` maps `dut` from its own
/// range, so `dut` stores its bounds when it has any. The wav `table` holds
/// [`wavetable`].
pub fn build(
    src_mode: ProcessingMode,
    dut_mode: ProcessingMode,
    module_type: &str,
    params: Value,
) -> Patch {
    let deserializers = get_params_deserializers();
    let freqs: Vec<f32> = (0..LANES).map(|lane| lane as f32 * 0.3).collect();
    let lfo_freqs: Vec<f32> = (0..LANES).map(|lane| lane as f32 * 0.2 - 3.0).collect();
    let dut_port = default_port(module_type);
    let modules = [
        ("src", "$saw", json!({ "freq": freqs }), src_mode),
        ("lfo", "$sine", json!({ "freq": lfo_freqs }), src_mode),
        ("dut", module_type, params, dut_mode),
        (
            "reader",
            "$remap",
            json!({
                "input": { "type": "cable", "module": "dut", "port": dut_port, "lane": 0 },
                "outMin": 0.0,
                "outMax": 1.0,
            }),
            ProcessingMode::Block,
        ),
    ];
    let mut patch = Patch::new();
    patch
        .insert_modules(
            modules.into_iter().map(|(id, module_type, params, mode)| {
                let cached = deserializers[module_type](params).unwrap();
                (
                    id.to_string(),
                    module_type.to_string(),
                    DeserializedParams {
                        params: cached.params,
                        lane_count: cached.lane_count,
                        f64_inputs: false,
                    },
                    mode,
                )
            }),
            SAMPLE_RATE,
            BLOCK_SIZE,
        )
        .unwrap();
    patch.wav_data.insert("table".into(), wavetable());
    for module in patch.sampleables.values() {
        module.prepare_resources(&patch.wav_data);
    }
    patch.connect_all();
    patch
}

/// A 16-frame wavetable of 256-sample cycles, each frame brighter than the last.
pub fn wavetable() -> Arc<WavData> {
    const SIZE: usize = 256;
    let samples = (0..16)
        .flat_map(|frame| {
            (0..SIZE).map(move |i| {
                let phase = std::f32::consts::TAU * i as f32 / SIZE as f32;
                (phase.sin() + (phase * (2 + frame) as f32).sin() * 0.3) * 0.7
            })
        })
        .collect();
    Arc::new(WavData::new(
        SampleBuffer::from_samples(vec![samples], 48_000.0),
        Some(SIZE),
    ))
}

/// Advance every module through `steps` (ascending slot targets ending at
/// `BLOCK_SIZE`) for one block, `dut` first so its reads pull unrendered
/// producers.
fn render_block(patch: &Patch, steps: &[usize]) {
    for module in patch.sampleables.values() {
        module.start_block();
    }
    for &step in steps {
        for id in ["dut", "src", "lfo"] {
            patch.sampleables[id].ensure_processed_to(step);
        }
    }
}

/// Render `BLOCKS` blocks through `steps`. Returns `(dut, src, lfo)` samples,
/// slot major then lane.
pub fn render(patch: &Patch, steps: &[usize]) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let (dut, src, lfo, _) = render_with_bounds(patch, steps);
    (dut, src, lfo)
}

/// [`render`], with `dut`'s bounds beside its samples.
pub fn render_with_bounds(
    patch: &Patch,
    steps: &[usize],
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<Bounds>) {
    let dut = &patch.sampleables["dut"];
    let dut_port = default_port(dut.get_module_type());
    let mut out = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for _ in 0..BLOCKS {
        render_block(patch, steps);
        for index in 0..BLOCK_SIZE {
            for lane in 0..LANES {
                let read =
                    |id: &str, port: &str| patch.sampleables[id].get_value_at(port, lane, index);
                out.0.push(read("dut", &dut_port));
                out.1.push(read("src", "output"));
                out.2.push(read("lfo", "output"));
                out.3.push(dut.get_bounds_at(&dut_port, lane, index));
            }
        }
    }
    out
}

/// `value` repeated for every rendered sample.
pub fn constant(value: f32) -> Vec<f32> {
    vec![value; BLOCKS * BLOCK_SIZE * LANES]
}

/// `f(src, lfo)` per sample, silencing non-finite results like every output.
pub fn elementwise(src: &[f32], lfo: &[f32], f: impl Fn(f32, f32) -> f32) -> Vec<f32> {
    src.iter()
        .zip(lfo)
        .map(|(&x, &y)| f(x, y))
        .map(|v| if v.is_finite() { v } else { 0.0 })
        .collect()
}

const STEP_PATTERNS: [&[usize]; 2] = [&[BLOCK_SIZE], &[1, 10, 11, 40, BLOCK_SIZE]];

/// Every sample of `dut` under each processing mode and step pattern, checked
/// against `reference(src, lfo)`.
pub fn check_module(
    module_type: &str,
    params: Value,
    reference: impl Fn(&[f32], &[f32]) -> Vec<f32>,
) {
    let modes = [
        (ProcessingMode::Block, ProcessingMode::Block),
        (ProcessingMode::Sample, ProcessingMode::Block),
        (ProcessingMode::Block, ProcessingMode::Sample),
    ];
    for (src_mode, dut_mode) in modes {
        for steps in STEP_PATTERNS {
            let patch = build(src_mode, dut_mode, module_type, params.clone());
            let (dut, src, lfo) = render(&patch, steps);
            assert_eq!(
                dut,
                reference(&src, &lfo),
                "{module_type}: src {src_mode:?}, dut {dut_mode:?}, steps {steps:?}"
            );
        }
    }
}

/// Every sample of `dut` under each processing mode and step pattern matches
/// the per-sample `update` path (`dut` in Sample mode).
pub fn check_paths_agree(module_type: &str, params: Value) {
    let reference = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        module_type,
        params.clone(),
    );
    let (expected, _, _) = render(&reference, &[BLOCK_SIZE]);
    assert!(
        expected.iter().any(|&v| v != 0.0),
        "{module_type} {params} renders silence"
    );
    let modes = [
        (ProcessingMode::Block, ProcessingMode::Block),
        (ProcessingMode::Sample, ProcessingMode::Block),
    ];
    for (src_mode, dut_mode) in modes {
        for steps in STEP_PATTERNS {
            let patch = build(src_mode, dut_mode, module_type, params.clone());
            let (dut, _, _) = render(&patch, steps);
            assert_eq!(
                dut, expected,
                "{module_type} {params}: src {src_mode:?}, steps {steps:?}"
            );
        }
    }
}

/// `dut`'s bounds over `BLOCKS` blocks rendered through `steps`, slot major
/// then lane, read as [`render`] reads values.
pub fn render_bounds(patch: &Patch, steps: &[usize]) -> Vec<Bounds> {
    render_with_bounds(patch, steps).3
}

/// `dut`'s bounds under each processing mode and step pattern match the
/// per-sample `update` path's, and are bounded. Returns them.
pub fn check_bounds_agree(module_type: &str, params: Value) -> Vec<Bounds> {
    let reference = build(
        ProcessingMode::Block,
        ProcessingMode::Sample,
        module_type,
        params.clone(),
    );
    let expected = render_bounds(&reference, &[BLOCK_SIZE]);
    assert!(
        expected.iter().all(|b| b.is_bounded()),
        "{module_type} {params} is unbounded"
    );
    let modes = [
        (ProcessingMode::Block, ProcessingMode::Block),
        (ProcessingMode::Sample, ProcessingMode::Block),
    ];
    for (src_mode, dut_mode) in modes {
        for steps in STEP_PATTERNS {
            let patch = build(src_mode, dut_mode, module_type, params.clone());
            assert_eq!(
                render_bounds(&patch, steps),
                expected,
                "{module_type} {params}: src {src_mode:?}, steps {steps:?}"
            );
        }
    }
    expected
}

/// `dut` swapped from `before` to `after` params after `BLOCKS` blocks, as the
/// audio thread swaps patches, then rendered on: the block path matches the
/// per-sample path, samples and bounds, before, through and after any glide
/// the swap starts.
pub fn check_swap_agrees(module_type: &str, before: Value, after: Value) {
    let run = |dut_mode: ProcessingMode, steps: &[usize]| {
        let old = build(ProcessingMode::Block, dut_mode, module_type, before.clone());
        let (mut out, _, _, mut bounds) = render_with_bounds(&old, steps);
        let new = build(ProcessingMode::Block, dut_mode, module_type, after.clone());
        for (id, module) in &new.sampleables {
            module.transfer_state_from(old.sampleables[id].as_ref(), &SameIds);
        }
        new.connect_all();
        let (after_out, _, _, after_bounds) = render_with_bounds(&new, steps);
        out.extend(after_out);
        bounds.extend(after_bounds);
        (out, bounds)
    };
    let expected = run(ProcessingMode::Sample, &[BLOCK_SIZE]);
    for steps in STEP_PATTERNS {
        assert_eq!(
            run(ProcessingMode::Block, steps),
            expected,
            "{module_type} {before} → {after}: steps {steps:?}"
        );
    }
}

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

/// Counts heap allocations and frees made on a thread while it is inside
/// [`allocations_while_rendering`]; other threads (parallel tests) are not
/// counted. A test binary opts in with
/// `#[global_allocator] static GLOBAL: CountingAllocator = CountingAllocator;`.
pub struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.with(Cell::get) {
            ALLOCATIONS.with(|n| n.set(n.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNTING.with(Cell::get) {
            ALLOCATIONS.with(|n| n.set(n.get() + 1));
        }
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// Heap allocations and frees made while rendering `dut` (built outside the
/// count) for several blocks in Block mode, from Sample-mode producers, and
/// in Sample mode itself, through every step pattern. Requires the binary's
/// global allocator to be [`CountingAllocator`].
pub fn allocations_while_rendering(module_type: &str, params: Value) -> usize {
    let modes = [
        (ProcessingMode::Block, ProcessingMode::Block),
        (ProcessingMode::Sample, ProcessingMode::Block),
        (ProcessingMode::Block, ProcessingMode::Sample),
    ];
    let patches: Vec<Patch> = modes
        .iter()
        .map(|&(src_mode, dut_mode)| build(src_mode, dut_mode, module_type, params.clone()))
        .collect();
    ALLOCATIONS.with(|n| n.set(0));
    COUNTING.with(|c| c.set(true));
    for patch in &patches {
        for steps in STEP_PATTERNS {
            for _ in 0..BLOCKS {
                render_block(patch, steps);
            }
        }
    }
    COUNTING.with(|c| c.set(false));
    ALLOCATIONS.with(Cell::get)
}
