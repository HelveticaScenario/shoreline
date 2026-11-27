//! Compiles NeuralAmpModelerCore and the C shim into one static library.

use std::path::{Path, PathBuf};

fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
        .collect();
    files.sort();
    files
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let core = manifest.join("NeuralAmpModelerCore");
    let nam = core.join("NAM");
    if !nam.join("dsp.cpp").exists() {
        panic!("NeuralAmpModelerCore is missing: run `git submodule update --init --recursive`");
    }

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .include(&core)
        .include(core.join("Dependencies/eigen"))
        .include(core.join("Dependencies/nlohmann"))
        .define("NAM_ENABLE_A2_FAST", None)
        // Eigen at -O0 runs far slower than real time, so the model is
        // optimised in every profile.
        .opt_level(3)
        .warnings(false)
        .files(sources(&nam))
        .files(sources(&nam.join("wavenet")))
        .file(manifest.join("shim/nam_shim.cpp"));

    if std::env::var_os("CARGO_FEATURE_ALLOC_TRACKING").is_some() {
        build
            .include(core.join("tools/test"))
            .file(core.join("tools/test/allocation_tracking.cpp"))
            .file(manifest.join("shim/alloc_tracking_shim.cpp"));
    }

    if std::env::var("PROFILE").as_deref() == Ok("release") {
        build.define("NDEBUG", None);
    }

    let json_assert = manifest.join("shim/json_assert.h");
    let target = std::env::var("TARGET").expect("target");
    if target.contains("msvc") {
        build
            .flag(format!("/FI{}", json_assert.display()))
            .define("NOMINMAX", None)
            .define("WIN32_LEAN_AND_MEAN", None)
            .flag("/EHsc")
            .flag("/bigobj");
    } else {
        build
            .flag("-include")
            .flag(json_assert.display().to_string())
            .flag("-fvisibility=hidden")
            // nlohmann also asserts inside `noexcept` functions, where a
            // failure terminates instead of reading out of bounds.
            .flag_if_supported("-Wno-exceptions")
            .flag_if_supported("-Wno-terminate");
        // Matches the x86-64-v2 baseline the Rust code is built for.
        if target.starts_with("x86_64") {
            build.flag("-march=x86-64-v2");
        }
        if target.contains("linux") && !build.get_compiler().is_like_clang() {
            build.flag("-fno-gnu-unique");
        }
    }

    // Each architecture registers itself from a static initialiser that
    // nothing references, so the whole archive is linked or the
    // registrations are dropped.
    build.link_lib_modifier("+whole-archive");
    build.compile("nam_core");

    println!("cargo:rerun-if-changed=shim");
    println!("cargo:rerun-if-changed=NeuralAmpModelerCore/NAM");
}
