//! Times NeuralAmpModelerCore's example models at several call sizes.
//!
//! `cargo run --release -p nam_sys --example bench`

use std::path::PathBuf;
use std::time::Instant;

use nam_sys::{NamDsp, NamFile};

const RATE: f64 = 48_000.0;
const SECONDS: usize = 10;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("NeuralAmpModelerCore/example_models");
    let len = RATE as usize * SECONDS;
    let input: Vec<f32> = (0..len)
        .map(|i| 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 110.0 / RATE as f32).sin())
        .collect();
    let mut output = vec![0.0f32; len];
    println!(
        "{:<28} {:>6} {:>10} {:>10}",
        "model", "block", "ns/sample", "% of core"
    );
    for name in ["A2.nam", "wavenet_a1_standard.nam", "lstm.nam"] {
        let file = NamFile::open(&dir.join(name)).expect("open model");
        for block in [1usize, 32, 128, 512] {
            let mut dsp = NamDsp::new(&file, RATE, block).expect("build");
            let run = if block == 1 { len / 10 } else { len };
            let start = Instant::now();
            for (x, y) in input[..run]
                .chunks(block)
                .zip(output[..run].chunks_mut(block))
            {
                assert!(dsp.process(x, y));
            }
            let ns = start.elapsed().as_nanos() as f64 / run as f64;
            println!(
                "{name:<28} {block:>6} {ns:>10.1} {:>9.1}%",
                ns * RATE / 1e9 * 100.0
            );
        }
    }
    std::hint::black_box(&output);
}
