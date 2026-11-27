//! `NamDsp::process` clear of the heap. The tracker counts every malloc and
//! free in the process, so this binary holds one test and nothing else runs
//! beside it.
#![cfg(feature = "alloc-tracking")]

use std::path::PathBuf;

use nam_sys::{NamDsp, NamFile};

const MODELS: &[&str] = &[
    "A2.nam",
    "wavenet_a1_standard.nam",
    "lstm.nam",
    "wavenet.nam",
    "slimmable_wavenet.nam",
    "slimmable_container.nam",
];

fn model(name: &str) -> NamFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("NeuralAmpModelerCore/example_models")
        .join(name);
    NamFile::open(&path).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn process_never_allocates() {
    let file = model("A2.nam");
    let built = nam_sys::count_allocations(|| drop(NamDsp::new(&file, 48_000.0, 128)));
    assert!(built > 0, "the tracker must see the runtime's allocations");
    for (name, size) in MODELS
        .iter()
        .flat_map(|name| [None, Some(0.0), Some(1.0)].map(|size| (name, size)))
    {
        let file = model(name);
        let mut dsp = NamDsp::new(&file, 48_000.0, 128).expect("build");
        if let Some(size) = size {
            dsp.set_size(size);
        }
        let input: Vec<f32> = (0..1_000).map(|i| (i as f32 * 0.01).sin() * 0.3).collect();
        let mut output = vec![0.0; input.len()];
        let mut ok = true;
        let count = nam_sys::count_allocations(|| {
            ok &= dsp.process(&input, &mut output);
            for (x, y) in input.iter().zip(output.iter_mut()) {
                ok &= dsp.process(std::slice::from_ref(x), std::slice::from_mut(y));
            }
        });
        assert!(ok, "{name} {size:?}");
        assert_eq!(
            count, 0,
            "{name} at size {size:?} allocated or freed {count} times while processing"
        );
    }
}
