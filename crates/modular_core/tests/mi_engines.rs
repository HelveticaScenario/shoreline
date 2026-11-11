//! The `$mi.*` modules against Plaits' engines (`engines.f32`, rendered by
//! `scripts/mi-reference/drivers/engines.cc` from each unmodified engine and
//! its output gain): the engine's knobs map onto the module's params, and its
//! `out` and `aux` reach the same level, spectrum and polarity once both have
//! settled. String and modal draw the same grains as Plaits, from the seed
//! the test pins. The per-sample fixtures compare waveforms; this pins the
//! knob mappings and output gains.

use modular_core::Patch;
use modular_core::dsp::get_params_deserializers;
use modular_core::params::DeserializedParams;
use modular_core::types::ProcessingMode;
use serde_json::{Value, json};

const SAMPLE_RATE: f32 = 48000.0;
const BLOCK: usize = 64;
/// Plaits tunes its notes to its codec's rate, 47,872.34 Hz, so rendered at
/// 48 kHz its pitches sit this many octaves above the modules'.
fn plaits_tuning() -> f32 {
    (48000.0f32 / 47872.34).log2()
}

/// Samples before the comparison, while controls and spectra settle.
const SETTLE: usize = 4000;

/// One output of a module set up as a Plaits case.
struct Port {
    module: &'static str,
    params: Value,
    port: &'static str,
}

/// The module outputs standing in for Plaits' `out` and `aux` in `engine` at
/// these knobs (0–1), at `freq` V/Oct.
fn ports(engine: i32, freq: f32, h: f32, t: f32, m: f32) -> [Port; 2] {
    let port = |module, params: Value, port| {
        let mut params = params;
        params["freq"] = json!(freq);
        Port {
            module,
            params,
            port,
        }
    };
    match engine {
        1 => {
            let knobs = |sync| json!({ "ratio": 5.0 * h, "amount": 5.0 * t, "asymmetry": 5.0 * m, "sync": sync });
            [
                port("$mi.phaseDistortion", knobs(true), "output"),
                port("$mi.phaseDistortion", knobs(false), "output"),
            ]
        }
        2..=4 => {
            // Plaits picks the patch from `harmonics`; the driver centres it on
            // one. The cases scrub (trigger unpatched), where Plaits' second
            // voice never gates and the module's one voice sounds alike.
            let knobs = json!({ "bank": engine - 2, "patch": (h * 32.64).floor(), "brightness": 5.0 * t, "envelope": 5.0 * m });
            [
                port("$mi.sixOp", knobs.clone(), "output"),
                port("$mi.sixOp", knobs, "output"),
            ]
        }
        5 => {
            let knobs = json!({ "terrain": 5.0 * h, "radius": 5.0 * t, "offset": 5.0 * m });
            [
                port("$mi.waveTerrain", knobs.clone(), "output"),
                port("$mi.waveTerrain", knobs, "pm"),
            ]
        }
        9 => {
            let knobs = json!({ "shape": 5.0 * h, "fold": 5.0 * t, "asymmetry": 5.0 * m });
            [
                port("$mi.waveshaper", knobs.clone(), "output"),
                port("$mi.waveshaper", knobs, "overtones"),
            ]
        }
        10 => {
            let knobs =
                json!({ "ratio": 5.0 * h, "index": 5.0 * t, "feedback": (2.0 * m - 1.0) * 5.0 });
            [
                port("$mi.twoOp", knobs.clone(), "output"),
                port("$mi.twoOp", knobs, "subOctave"),
            ]
        }
        11 => [
            port(
                "$mi.formant",
                json!({ "formant": (24.0 + 84.0 * t - 60.0) / 12.0 + plaits_tuning(), "ratio": 5.0 * h, "width": 5.0 * m }),
                "output",
            ),
            port(
                "$mi.zOsc",
                json!({ "cutoff": 5.0 * t, "shape": 5.0 * m, "mode": 5.0 * h }),
                "output",
            ),
        ],
        12 => {
            let knobs = |series| json!({ "bumps": 5.0 * h, "centroid": 5.0 * t, "slope": 5.0 * m, "series": series });
            [
                port("$mi.additive", knobs("integer"), "output"),
                port("$mi.additive", knobs("organ"), "output"),
            ]
        }
        15 => {
            // The vowel path, where harmonics sounds one synth alone: the
            // formant filters at 0, SAM at 1/6 and LPC just below 1/3.
            let module = match h {
                h if h < 0.1 => "$mi.formantVowel",
                h if h < 0.25 => "$mi.samVowel",
                _ => "$mi.lpcVowel",
            };
            let knobs = json!({ "vowel": 5.0 * m, "formantShift": 5.0 * t });
            [
                port(module, knobs.clone(), "output"),
                port(module, knobs, "excitation"),
            ]
        }
        16 => {
            let knobs = json!({ "density": 5.0 * t, "spread": 5.0 * h, "grainSize": 5.0 * m });
            [
                port("$mi.swarm", knobs.clone(), "output"),
                port("$mi.swarm", knobs, "sine"),
            ]
        }
        18 => {
            let knobs = json!({ "density": 5.0 * t, "spread": 5.0 * h, "resonance": 5.0 * m });
            [
                port("$mi.particles", knobs.clone(), "output"),
                port("$mi.particles", knobs, "raw"),
            ]
        }
        19 | 20 => {
            let module = if engine == 19 {
                "$mi.string"
            } else {
                "$mi.modal"
            };
            let knobs = json!({ "structure": 5.0 * h, "brightness": 5.0 * t, "decay": 5.0 * m });
            [
                port(module, knobs.clone(), "output"),
                port(module, knobs, "exciter"),
            ]
        }
        _ => panic!("no module for engine {engine}"),
    }
}

/// `samples` of `port`, in volts.
fn render(port: &Port, samples: usize) -> Vec<f32> {
    let deserializers = get_params_deserializers();
    let cached = deserializers[port.module](port.params.clone()).unwrap();
    let mut patch = Patch::new();
    patch
        .insert_modules(
            [(
                "dut".to_string(),
                port.module.to_string(),
                DeserializedParams {
                    params: cached.params,
                    channel_count: cached.channel_count,
                },
                ProcessingMode::Block,
            )],
            SAMPLE_RATE,
            BLOCK,
        )
        .unwrap();
    patch.connect_all();
    let mut out = Vec::with_capacity(samples);
    while out.len() < samples {
        let dut = &patch.sampleables["dut"];
        dut.start_block();
        dut.ensure_processed_to(BLOCK);
        for index in 0..BLOCK {
            out.push(dut.get_value_at(port.port, 0, index));
        }
    }
    out.truncate(samples);
    out
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// The rms difference between `actual` and `expected` over the loudest 1024
/// samples of `expected` after `SETTLE`, at the lag within ±48 samples, in
/// eighths of a sample, that aligns them best: oscillators that start their
/// phase differently line up only between samples.
fn aligned_error(actual: &[f32], expected: &[f32]) -> f32 {
    let start = (SETTLE..expected.len() - 1024 - 48)
        .step_by(256)
        .max_by(|&a, &b| rms(&expected[a..a + 1024]).total_cmp(&rms(&expected[b..b + 1024])))
        .unwrap();
    let window = start..start + 1024;
    (-48 * 8..=48 * 8)
        .map(|eighths: i32| {
            let whole = eighths.div_euclid(8);
            let fraction = eighths.rem_euclid(8) as f32 / 8.0;
            let error: f32 = window
                .clone()
                .map(|n| {
                    let i = (n as i32 + whole) as usize;
                    let a = actual[i] + (actual[i + 1] - actual[i]) * fraction;
                    let d = a - expected[n];
                    d * d
                })
                .sum();
            (error / window.len() as f32).sqrt()
        })
        .fold(f32::INFINITY, f32::min)
}

/// In-place radix-2 FFT of `re`/`im` (length a power of two).
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -std::f64::consts::TAU / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((angle * k as f64).cos(), (angle * k as f64).sin());
                let (a, b) = (start + k, start + k + len / 2);
                let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
        }
        len <<= 1;
    }
}

/// Level in each octave band from 62.5 Hz to 16 kHz, in dB, from Hann-windowed
/// 4096-sample spectra averaged across the signal after `SETTLE`.
fn bands(x: &[f32]) -> Vec<f32> {
    const N: usize = 4096;
    let mut power = vec![0.0f64; N / 2];
    let mut frames = 0;
    for start in (SETTLE..x.len() - N).step_by(N / 2) {
        frames += 1;
        let mut re: Vec<f64> = x[start..start + N]
            .iter()
            .enumerate()
            .map(|(n, &v)| {
                f64::from(v) * (0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / N as f64).cos())
            })
            .collect();
        let mut im = vec![0.0; N];
        fft(&mut re, &mut im);
        for (k, p) in power.iter_mut().enumerate() {
            *p += re[k] * re[k] + im[k] * im[k];
        }
    }
    let hz_per_bin = f64::from(SAMPLE_RATE) / N as f64;
    (0..9)
        .map(|b| {
            let low = 62.5 * f64::from(1 << b);
            let sum: f64 = power
                .iter()
                .enumerate()
                .filter(|(k, _)| (low..2.0 * low).contains(&(*k as f64 * hz_per_bin)))
                .map(|(_, p)| p)
                .sum();
            (10.0 * (sum / f64::from(frames) + 1e-12).log10()) as f32
        })
        .collect()
}

/// Asserts `actual` has `expected`'s level within `level_db`, and the same
/// level in each octave band within `near_db` where the band is within 10 dB
/// of the loudest and within `far_db` down to 40 dB below it.
fn assert_sounds_alike(
    name: &str,
    actual: &[f32],
    expected: &[f32],
    level_db: f32,
    near_db: f32,
    far_db: f32,
) {
    let level = 20.0 * (rms(&actual[SETTLE..]) / rms(&expected[SETTLE..])).log10();
    assert!(level.abs() < level_db, "{name}: level {level} dB");
    let expected = bands(expected);
    let loudest = expected.iter().fold(f32::MIN, |m, &b| m.max(b));
    for (b, (a, e)) in bands(actual).iter().zip(&expected).enumerate() {
        let tolerance = if e > &(loudest - 10.0) {
            near_db
        } else if e > &(loudest - 40.0) {
            far_db
        } else {
            continue;
        };
        assert!((a - e).abs() < tolerance, "{name}: band {b} {a} vs {e} dB");
    }
}

#[test]
fn matches_plaits_engines() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/mi/engines.f32"
    ))
    .expect("engines.f32: run scripts/mi-reference/build.sh, or git lfs pull");
    let floats: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    // The seed `engines.cc` starts Plaits' random generator from.
    modular_core::dsp::utils::rng::TEST_SEED_BASE.set(Some(1));
    let mut rest = &floats[..];
    while !rest.is_empty() {
        let [engine, note, h, t, m, samples] = rest[..6] else {
            unreachable!()
        };
        let samples = samples as usize;
        let case = &rest[6..6 + 2 * samples];
        rest = &rest[6 + 2 * samples..];
        let engine = engine as i32;
        let freq = (note - 60.0) / 12.0 + plaits_tuning();
        for (output, port) in ports(engine, freq, h, t, m).iter().enumerate() {
            let expected: Vec<f32> = case.iter().skip(output).step_by(2).copied().collect();
            let actual = render(port, samples);
            let name = format!(
                "engine {engine} note {note} knobs {h}/{t}/{m}: {} {}",
                port.module, port.port
            );
            let inverted: Vec<f32> = actual.iter().map(|v| -v).collect();
            assert_sounds_alike(&name, &actual, &expected, 0.1, 0.6, 3.0);
            // Particles draw Plaits' values in Plaits' order on its block
            // timing, so they match its engine sample for sample, to the
            // rounding of a pitch given in volts.
            if engine == 18 {
                let error = actual
                    .iter()
                    .zip(&expected)
                    .fold(0.0f32, |m, (a, e)| m.max((a - e).abs()));
                assert!(error < 1e-4, "{name}: max error {error}");
            }
            assert!(
                aligned_error(&actual, &expected) < aligned_error(&inverted, &expected),
                "{name}: inverted"
            );
        }
    }
}
