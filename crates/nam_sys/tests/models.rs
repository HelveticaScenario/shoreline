//! Runs NeuralAmpModelerCore's example models through the bindings.

use std::path::PathBuf;

use nam_sys::{NamDsp, NamFile};

fn model(name: &str) -> NamFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("NeuralAmpModelerCore/example_models")
        .join(name);
    NamFile::open(&path).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn sine(len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 110.0 / 48_000.0).sin())
        .collect()
}

const MODELS: &[&str] = &[
    "A2.nam",
    "wavenet_a1_standard.nam",
    "lstm.nam",
    "wavenet.nam",
    "slimmable_wavenet.nam",
    "slimmable_container.nam",
];

#[test]
fn every_example_model_builds_and_sounds() {
    for name in MODELS {
        let file = model(name);
        let mut dsp = NamDsp::new(&file, 48_000.0, 256).unwrap_or_else(|e| panic!("{name}: {e}"));
        let input = sine(4_800);
        let mut output = vec![0.0; input.len()];
        assert!(dsp.process(&input, &mut output), "{name}");
        assert!(output.iter().all(|y| y.is_finite()), "{name}");
        assert!(output.iter().any(|y| y.abs() > 1e-4), "{name} is silent");
    }
}

/// Writes `json` to a scratch file and opens it as a model.
fn open_text(name: &str, json: &str) -> Result<NamDsp, String> {
    let path = std::env::temp_dir().join(format!("nam_sys_{}_{name}.nam", std::process::id()));
    std::fs::write(&path, json).expect("write scratch model");
    let built = NamFile::open(&path).and_then(|file| NamDsp::new(&file, 48_000.0, 64));
    let _ = std::fs::remove_file(&path);
    built
}

#[test]
fn malformed_files_are_errors() {
    assert!(NamFile::open(std::path::Path::new("/no/such/model.nam")).is_err());
    assert!(open_text("not_json", "{").is_err());
    assert!(open_text("no_config", r#"{"version": "0.5.0"}"#).is_err());
    let standard = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("NeuralAmpModelerCore/example_models/wavenet_a1_standard.nam"),
    )
    .expect("read model");
    let nested = standard.replacen("\"channels\"", "\"chanels\"", 1);
    assert!(open_text("nested_field", &nested).is_err());
    let truncated = standard.replacen("\"weights\": [", "\"weights\": [0.5], \"unused\": [", 1);
    assert!(open_text("few_weights", &truncated).is_err());
}

/// The block path and the one-sample path read the same model; this reports
/// how far apart they land.
#[test]
fn one_sample_calls_match_block_calls() {
    for name in MODELS {
        let file = model(name);
        let mut block = NamDsp::new(&file, 48_000.0, 128).expect("build");
        let mut single = NamDsp::new(&file, 48_000.0, 128).expect("build");
        let input = sine(2_048);
        let mut by_block = vec![0.0; input.len()];
        let mut by_sample = vec![0.0; input.len()];
        assert!(block.process(&input, &mut by_block));
        for (x, y) in input.iter().zip(by_sample.iter_mut()) {
            assert!(single.process(std::slice::from_ref(x), std::slice::from_mut(y)));
        }
        let max_diff = by_block
            .iter()
            .zip(&by_sample)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        println!("{name}: max |block - per-sample| = {max_diff:e}");
        assert!(max_diff < 1e-4, "{name}: {max_diff}");
    }
}
