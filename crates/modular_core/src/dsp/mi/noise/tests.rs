use super::*;
use crate::dsp::mi::core::fixture::{max_error, records};

fn new_lane(seed: u32) -> Lane {
    Lane {
        random: Random::new(seed),
        seeded: true,
        ..Lane::default()
    }
}

/// Replays `noise.f32` (Plaits' clocked noise and filters, a sample at a
/// time, then its limiters): the clock sweeping with the trigger unpatched,
/// the mode sweeping, the resonance sweeping, and a slow clock restarted by
/// syncs.
#[test]
fn matches_plaits() {
    let state = NoiseState::new(48000.0);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for records in records("noise", 6 + 2).chunks(9600) {
        let mut lane = new_lane(0x21);
        for record in records {
            let patched = record[5] > 0.5;
            lane.control(
                record[0],
                [record[1], record[2], record[3]],
                patched,
                &state,
            );
            let (out, aux) = lane.step(record[4] > 0.5, &state);
            actual.extend([out, aux]);
            expected.extend([record[6], record[7]]);
        }
    }
    let (error, at) = max_error(&actual, &expected);
    assert!(error < 1e-5, "max error {error} at sample {}", at / 2);
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// `seconds` of both outputs at `hz` with steady knobs, the trigger
/// unpatched, each through an 8-pole lowpass at 16 kHz, so a faster rate's
/// content above 24 kHz, which 48 kHz cannot hold, is left out.
fn render(sample_rate: f32, hz: f32, knobs: [f32; 3], seconds: f32) -> (Vec<f32>, Vec<f32>) {
    let state = NoiseState::new(sample_rate);
    let mut lane = new_lane(0x5eed);
    let mut lowpass = [Svf::default(); 8];
    for svf in &mut lowpass {
        svf.set_g_q(
            tan_accurate(16000.0 / sample_rate),
            std::f32::consts::FRAC_1_SQRT_2,
        );
    }
    (0..(sample_rate * seconds) as usize)
        .map(|_| {
            lane.control(hz / sample_rate, knobs, false, &state);
            let (out, aux) = lane.step(false, &state);
            let (out_lp, aux_lp) = lowpass.split_at_mut(4);
            let out = out_lp.iter_mut().fold(out, |x, svf| svf.low_pass(x));
            let aux = aux_lp.iter_mut().fold(aux, |x, svf| svf.low_pass(x));
            (out, aux)
        })
        .unzip()
}

/// Smooth hiss, stepped noise and every filter mode reach the same level below
/// 16 kHz at any rate, within 1 dB, at 440 Hz and in the top octaves at 8 kHz.
#[test]
fn sounds_the_same_at_any_rate() {
    for (hz, knobs) in [
        (440.0, [5.0, 2.5, 1.0]),
        (440.0, [5.0, 0.0, 3.0]),
        (440.0, [5.0, 5.0, 2.0]),
        (440.0, [2.0, 2.5, 1.0]),
        (440.0, [3.0, 1.0, 4.0]),
        (8000.0, [5.0, 2.5, 1.0]),
        (8000.0, [5.0, 0.0, 3.0]),
        (8000.0, [5.0, 5.0, 2.0]),
    ] {
        let (out, aux) = render(48000.0, hz, knobs, 4.0);
        for sample_rate in [44100.0, 96000.0] {
            let (rate_out, rate_aux) = render(sample_rate, hz, knobs, 4.0);
            for (name, a, r) in [("filtered", &rate_out, &out), ("bands", &rate_aux, &aux)] {
                let level = 20.0 * (rms(a) / rms(r)).log10();
                assert!(
                    level.abs() < 1.0,
                    "{hz} Hz {knobs:?} {name} at {sample_rate} Hz: level {level} dB"
                );
            }
        }
    }
}

/// The module's outputs are its lane's `filtered` and `bands` through Plaits'
/// output gain and clip, the lane seeded as `on_patch_update` seeds it.
#[test]
fn outputs_are_the_lane_through_the_output_gain() {
    use crate::poly::PolySignal;
    use crate::types::{OutputStruct, PatchUpdateHandler, Signal};
    let volts = |v: f32| Some(PolySignal::mono(Signal::Volts(v)));
    let params = MiNoiseParams {
        freq: PolySignal::mono(Signal::Volts(1.0)),
        trigger: None,
        rate: volts(4.0),
        mode: volts(1.5),
        resonance: volts(3.0),
        fm: None,
        fm_mode: FmMode::default(),
    };
    let mut outputs = MiNoiseOutputs::default();
    outputs.set_all_lanes(1);
    let mut module = MiNoise {
        params,
        outputs,
        state: NoiseState::default(),
        lane_state: vec![Lane::default()].into_boxed_slice(),
        _lane_count: 1,
        _channels: 1,
        _block_index: Default::default(),
    };
    module.init(48000.0);
    crate::dsp::utils::rng::TEST_SEED_BASE.set(Some(0x5eed));
    module.on_patch_update();
    crate::dsp::utils::rng::TEST_SEED_BASE.set(None);

    let state = NoiseState::new(48000.0);
    let unit = (hash_bipolar(0x5eed, 0) + 1.0) * 0.5;
    let mut lane = new_lane((unit * f64::from(u32::MAX)) as u32);
    let f0 = apply_fm(1.0, MiNoiseParams::FM_DEFAULT, FmMode::default()) * state.inv_sample_rate;
    for _ in 0..4800 {
        module.update(48000.0);
        lane.control(f0, [4.0, 1.5, 3.0], false, &state);
        let (out, aux) = lane.step(false, &state);
        assert_eq!(
            module.outputs.sample.get(0),
            (OUTPUT_GAIN * out).clamp(-5.0, 5.0)
        );
        assert_eq!(
            module.outputs.bands.get(0),
            (OUTPUT_GAIN * aux).clamp(-5.0, 5.0)
        );
    }
}

/// NaN and infinite knobs hold the last finite ones, and extreme pitches and
/// knobs stay finite.
#[test]
fn stays_finite() {
    let state = NoiseState::new(48000.0);
    let mut lane = new_lane(7);
    let knobs = [
        [5.0, 2.5, 5.0],
        [f32::NAN, f32::INFINITY, f32::NEG_INFINITY],
        [-100.0, 100.0, 100.0],
        [0.0, 0.0, 0.0],
    ];
    for f0 in [1.0 / 48000.0, 0.01, MAX_FILTER] {
        for (n, knobs) in knobs.iter().cycle().take(40000).enumerate() {
            lane.control(f0, *knobs, n % 3 == 0, &state);
            let (out, aux) = lane.step(n % 997 == 0, &state);
            assert!(out.is_finite() && aux.is_finite(), "{f0} {knobs:?}");
        }
    }
    let mut lane = new_lane(7);
    lane.control(0.01, [1.0, 2.0, 3.0], false, &state);
    let held = lane.held;
    lane.control(0.01, [f32::NAN, f32::INFINITY, f32::NAN], false, &state);
    assert_eq!(lane.held, held);
}
