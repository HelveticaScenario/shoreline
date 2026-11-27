//! Times one `.nam` file at the engine's block size, at each of its tiers.
//!
//! `cargo run --release -p nam_sys --example bench_file -- <path.nam>`

use std::time::Instant;

use nam_sys::{NamDsp, NamFile};

const RATE: f64 = 48_000.0;
const BLOCK: usize = 64;

fn main() {
    let path = std::env::args().nth(1).expect("path to a .nam file");
    let file = NamFile::open(path.as_ref()).expect("open model");
    let len = RATE as usize * 10;
    let input: Vec<f32> = (0..len)
        .map(|i| 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 110.0 / RATE as f32).sin())
        .collect();
    let mut output = vec![0.0f32; len];
    for size in [1.0, 0.0] {
        let start = Instant::now();
        let mut dsp = NamDsp::new(&file, RATE, BLOCK).expect("build");
        let tiered = dsp.set_size(size);
        let built = start.elapsed();
        let start = Instant::now();
        for (x, y) in input.chunks(BLOCK).zip(output.chunks_mut(BLOCK)) {
            assert!(dsp.process(x, y));
        }
        let ns = start.elapsed().as_nanos() as f64 / len as f64;
        println!(
            "size {size}: tiered {tiered}, build {:.1} ms, {ns:.1} ns/sample, {:.2}% of a core",
            built.as_secs_f64() * 1e3,
            ns * RATE / 1e9 * 100.0
        );
    }
    std::hint::black_box(&output);
}
