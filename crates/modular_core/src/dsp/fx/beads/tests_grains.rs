use super::super::tests::{SR, poly_volts, rms, sine};
use super::*;
use crate::types::{Connect, LaneExpansion, OutputStruct, PolySignalFields, Signal};

fn grains_params(quality: GranularQuality) -> GranularParams {
    GranularParams {
        input: PolySignal::mono(Signal::Volts(0.0)),
        trigger: poly_volts(&[0.0]),
        freeze: None,
        time: None,
        size: None,
        shape: None,
        freq: None,
        root_freq: None,
        spread: None,
        feedback: None,
        mix: None,
        quality,
        fresh: None,
        mono: false,
    }
}

/// Each value on both sides of its lane, as the patch expands a stereo
/// module's inputs.
fn sides(values: &[f32]) -> Option<PolySignal> {
    let mut signal = poly_volts(values);
    signal.expand_lanes(&LaneExpansion {
        channels: 2,
        producer: &|_, _| 1,
    });
    signal
}

/// Build a module the way the engine does: one voice per side of each lane
/// of the widest input, `init`, then `on_patch_update`.
fn make_grains(params: GranularParams, sample_rate: f32) -> Granular {
    let mut m = construct(params, sample_rate);
    m.on_patch_update();
    m
}

/// Carry `old`'s state into `new` as a patch update does: the whole module
/// state, and the voices both modules have.
fn transfer(old: &mut Granular, new: &mut Granular) {
    std::mem::swap(&mut old.state, &mut new.state);
    let n = old.lane_state.len().min(new.lane_state.len());
    new.lane_state[..n].swap_with_slice(&mut old.lane_state[..n]);
    new.on_patch_update();
}

/// A module up to the point where state transfer happens.
fn construct(mut params: GranularParams, sample_rate: f32) -> Granular {
    let lanes = 2 * params
        .poly_signal_fields()
        .iter()
        .map(|p| p.lanes())
        .max()
        .unwrap_or(1)
        .max(1);
    params.expand_lanes(&LaneExpansion {
        channels: 2,
        producer: &|_, _| 1,
    });
    let mut outputs = GranularOutputs::default();
    outputs.set_all_lanes(lanes);
    let mut m = Granular {
        outputs,
        state: Default::default(),
        params,
        lane_state: (0..lanes).map(|_| Default::default()).collect(),
        _lane_count: lanes,
        _channels: 2,
        _block_index: Default::default(),
        _connected: Default::default(),
    };
    m.init(sample_rate);
    m
}

/// Advance one sample with a mono input and trigger level; returns the
/// stereo output.
fn step(m: &mut Granular, input: f32, trigger: f32, sample_rate: f32) -> (f32, f32) {
    m.params.input = PolySignal::mono(Signal::Volts(input));
    m.params.trigger = poly_volts(&[trigger]);
    m.update(sample_rate);
    (m.outputs.sample.get(0), m.outputs.sample.get(1))
}

const ALL_QUALITIES: [GranularQuality; 4] = [
    GranularQuality::Digital,
    GranularQuality::Vintage,
    GranularQuality::CleanTape,
    GranularQuality::DirtyTape,
];

#[test]
fn silence_stays_silent_in_every_quality() {
    for q in ALL_QUALITIES {
        let mut m = make_grains(grains_params(q), SR);
        for n in 0..20_000 {
            let trig = if n % 2400 < 10 { 5.0 } else { 0.0 };
            let (l, r) = step(&mut m, 0.0, trig, SR);
            assert!(l.abs() < 1e-6 && r.abs() < 1e-6, "{q:?}: {l} {r} at {n}");
        }
    }
}

#[test]
fn dry_signal_passes_at_hardware_dry_gain() {
    // Fully dry, the hardware's dry/wet law leaves the input at 0.75×, and a
    // signal far past full scale passes unlimited.
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[0.0]);
    let mut m = make_grains(p, SR);
    for n in 0..4800 {
        let x = sine(220.0, n, SR, 15.0);
        let (l, r) = step(&mut m, x, 0.0, SR);
        if n > 100 {
            assert!((l - 0.75 * x).abs() < 1e-4, "left {l} vs {x} at {n}");
            assert!((r - 0.75 * x).abs() < 1e-4, "right {r} vs {x} at {n}");
        }
    }
}

#[test]
fn grains_play_only_after_a_trigger() {
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[5.0]);
    p.size = poly_volts(&[3.5]);
    let mut m = make_grains(p, SR);
    let mut before = Vec::new();
    let mut after = Vec::new();
    for n in 0..48_000 {
        let trig = if (24_000..24_010).contains(&n) {
            5.0
        } else {
            0.0
        };
        let (l, _) = step(&mut m, sine(330.0, n, SR, 2.0), trig, SR);
        if (12_000..24_000).contains(&n) {
            before.push(l);
        } else if (24_500..30_000).contains(&n) {
            after.push(l);
        }
    }
    assert!(
        rms(&before) < 1e-3,
        "wet output before any trigger: {}",
        rms(&before)
    );
    assert!(rms(&after) > 0.3, "grain output too quiet: {}", rms(&after));
}

/// Estimate frequency from the interpolated positions of the first and last
/// rising zero crossings.
fn frequency(xs: &[f32], sample_rate: f32) -> f32 {
    let crossings: Vec<f32> = xs
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
        .map(|(i, w)| i as f32 + w[0] / (w[0] - w[1]))
        .collect();
    let (first, last) = (crossings[0], crossings[crossings.len() - 1]);
    (crossings.len() - 1) as f32 * sample_rate / (last - first)
}

#[test]
fn freq_transposes_the_grain_from_root_freq_by_volts_per_octave() {
    // 440 Hz is A4, 0.75 V above C4.
    let a4 = (440.0f32 / 261.625_58).log2();
    for (freq, root, expected) in [
        (0.0, 0.0, 440.0),
        (1.0, 0.0, 880.0),
        (-1.0, 0.0, 220.0),
        (1.0, a4, 523.251),
        (a4, a4, 440.0),
    ] {
        let mut p = grains_params(GranularQuality::Digital);
        p.mix = poly_volts(&[5.0]);
        // An ~86 ms square-enveloped grain.
        p.size = poly_volts(&[2.0]);
        p.shape = poly_volts(&[0.0]);
        p.time = poly_volts(&[1.0]);
        p.freq = poly_volts(&[freq]);
        p.root_freq = poly_volts(&[root]);
        let mut m = make_grains(p, SR);
        let mut out = Vec::new();
        for n in 0..60_000 {
            let trig = if (48_000..48_010).contains(&n) {
                5.0
            } else {
                0.0
            };
            let (l, _) = step(&mut m, sine(440.0, n, SR, 2.0), trig, SR);
            if (48_500..51_500).contains(&n) {
                out.push(l);
            }
        }
        assert!(rms(&out) > 0.3, "freq {freq} root {root}: grain silent");
        let f = frequency(&out, SR);
        assert!(
            // Within the accuracy of the frequency estimate.
            (f - expected).abs() < expected * 0.002,
            "freq {freq} root {root}: {f} Hz"
        );
    }
}

#[test]
fn low_rate_qualities_band_limit_the_dry_path() {
    // Dirty tape runs at 24 kHz and vintage at 32 kHz: an 11.5 kHz tone is
    // above the dirty-tape passband but inside vintage's.
    for (q, passes) in [
        (GranularQuality::DirtyTape, false),
        (GranularQuality::Vintage, true),
    ] {
        let mut p = grains_params(q);
        p.mix = poly_volts(&[0.0]);
        let mut m = make_grains(p, SR);
        let mut out = Vec::new();
        for n in 0..24_000 {
            let (l, _) = step(&mut m, sine(11_500.0, n, SR, 1.0), 0.0, SR);
            if n > 4_000 {
                out.push(l);
            }
        }
        let gain = rms(&out) / (1.0 / std::f32::consts::SQRT_2);
        if passes {
            assert!(gain > 0.6, "{q:?} gain {gain}");
        } else {
            assert!(gain < 0.05, "{q:?} gain {gain}");
        }
    }
}

#[test]
fn resampled_dry_path_keeps_its_level() {
    for (q, sample_rate) in [
        (GranularQuality::DirtyTape, 48_000.0),
        (GranularQuality::Vintage, 48_000.0),
        (GranularQuality::Digital, 44_100.0),
    ] {
        let mut p = grains_params(q);
        p.mix = poly_volts(&[0.0]);
        let mut m = make_grains(p, sample_rate);
        let mut out = Vec::new();
        for n in 0..(sample_rate as usize) {
            let (l, _) = step(&mut m, sine(1000.0, n, sample_rate, 1.0), 0.0, sample_rate);
            if n > 4_000 {
                out.push(l);
            }
        }
        let gain = rms(&out) / (1.0 / std::f32::consts::SQRT_2);
        assert!(
            (gain - 0.75).abs() < 0.03,
            "{q:?} at {sample_rate}: gain {gain}"
        );
    }
}

#[test]
fn freeze_keeps_replaying_the_held_audio() {
    let run = |frozen: bool| {
        let mut p = grains_params(GranularQuality::Digital);
        p.mix = poly_volts(&[5.0]);
        p.size = poly_volts(&[3.5]);
        let mut m = make_grains(p, SR);
        let mut out = Vec::new();
        for n in 0..96_000 {
            // Audio for the first second, then silence.
            let x = if n < 48_000 {
                sine(330.0, n, SR, 2.0)
            } else {
                0.0
            };
            m.params.freeze = poly_volts(&[if frozen && n >= 47_000 { 5.0 } else { 0.0 }]);
            let trig = if n % 4800 < 10 { 5.0 } else { 0.0 };
            let (l, _) = step(&mut m, x, trig, SR);
            if n >= 72_000 {
                out.push(l);
            }
        }
        rms(&out)
    };
    let held = run(true);
    let released = run(false);
    assert!(held > 0.3, "frozen grains silent: {held}");
    assert!(
        released < 1e-3,
        "grains of silence should be silent: {released}"
    );
}

/// Record a second of a sine into every voice.
fn record(m: &mut Granular) {
    for n in 0..48_000 {
        step(m, sine(220.0, n, SR, 1.0), 0.0, SR);
    }
}

fn recording_level(m: &Granular, voice: usize) -> usize {
    let buffer = &m.lane_state[voice].voice.granular.buffer;
    (0..buffer.max_delay())
        .filter(|&i| buffer.raw(i as usize) != 0.0)
        .count()
}

#[test]
fn a_quality_change_takes_a_fresh_buffer_of_the_new_length() {
    let mut old = make_grains(grains_params(GranularQuality::Digital), SR);
    record(&mut old);
    assert!(recording_level(&old, 0) > 0);
    let mut new = construct(grains_params(GranularQuality::DirtyTape), SR);
    transfer(&mut old, &mut new);
    assert_eq!(new.state.media, Media::DirtyTape);
    let buffer = &new.lane_state[0].voice.granular.buffer;
    assert_eq!(buffer.max_delay() as usize, Media::DirtyTape.max_delay());
    assert_eq!(recording_level(&new, 0), 0);
    for n in 0..4800 {
        let (l, r) = step(&mut new, sine(220.0, n, SR, 1.0), 0.0, SR);
        assert!(l.is_finite() && r.is_finite());
    }
}

#[test]
fn an_unchanged_quality_keeps_the_recording() {
    let mut old = make_grains(grains_params(GranularQuality::DirtyTape), SR);
    record(&mut old);
    let recorded = recording_level(&old, 0);
    assert!(recorded > 0);
    let mut new = construct(grains_params(GranularQuality::DirtyTape), SR);
    transfer(&mut old, &mut new);
    assert_eq!(recording_level(&new, 0), recorded);
    // Later patch updates keep it too.
    new.on_patch_update();
    assert_eq!(recording_level(&new, 0), recorded);
}

#[test]
fn a_fresh_dirty_tape_buffer_plays_silence() {
    let mut p = grains_params(GranularQuality::DirtyTape);
    p.mix = poly_volts(&[5.0]);
    p.time = poly_volts(&[5.0]);
    let mut m = make_grains(p, SR);
    for n in 0..24_000 {
        let trig = if n % 2400 < 10 { 5.0 } else { 0.0 };
        let (l, r) = step(&mut m, 0.0, trig, SR);
        assert!(l.abs() < 1e-6 && r.abs() < 1e-6, "{l} {r} at {n}");
    }
}

#[test]
fn new_voices_get_their_own_buffers() {
    let mut old = make_grains(grains_params(GranularQuality::Digital), SR);
    record(&mut old);
    let mut p = grains_params(GranularQuality::Digital);
    p.freq = poly_volts(&[0.0, 0.5]);
    let mut new = construct(p, SR);
    transfer(&mut old, &mut new);
    // Each lane plays from its left voice, which keeps its recording or
    // takes a fresh one.
    assert!(recording_level(&new, 0) > 0);
    assert!(!records(&new, 1));
    assert_eq!(recording_level(&new, 2), 0);
    let buffer = &new.lane_state[2].voice.granular.buffer;
    assert_eq!(buffer.max_delay() as usize, Media::Digital.max_delay());
}

#[test]
fn narrowing_drops_triggers_for_voices_that_are_gone() {
    // At 44.1 kHz triggers wait for the resampled input, so some are still
    // queued at the patch update.
    let sr = 44_100.0;
    let mut p = grains_params(GranularQuality::Digital);
    p.freq = poly_volts(&[0.0, 0.25, 0.5, 0.75]);
    p.mix = poly_volts(&[5.0]);
    let mut old = make_grains(p, sr);
    for n in 0..4_000 {
        old.params.input = PolySignal::mono(Signal::Volts(sine(220.0, n, sr, 1.0)));
        old.params.trigger = poly_volts(&[if n % 7 == 0 { 5.0 } else { 0.0 }]);
        old.update(sr);
    }
    old.params.trigger = poly_volts(&[5.0]);
    old.update(sr);
    assert!(!old.state.pending.is_empty());
    let mut new = construct(grains_params(GranularQuality::Digital), sr);
    transfer(&mut old, &mut new);
    assert_eq!(new.lane_state.len(), 2);
    for n in 0..4_000 {
        let (l, r) = step(&mut new, sine(220.0, n, sr, 1.0), 0.0, sr);
        assert!(l.is_finite() && r.is_finite());
    }
}

#[test]
fn recovers_after_non_finite_input() {
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[2.5]);
    let mut m = make_grains(p, SR);
    for _ in 0..64 {
        step(&mut m, f32::NAN, 0.0, SR);
    }
    for n in 0..4800 {
        let (l, r) = step(&mut m, sine(220.0, n, SR, 1.0), 0.0, SR);
        assert!(l.is_finite() && r.is_finite(), "non-finite output at {n}");
    }
    assert!(m.outputs.sample.get(0).abs() > 0.0);
}

#[test]
fn unspread_grains_of_a_mono_input_play_alike_on_both_sides() {
    for quality in ALL_QUALITIES {
        let mut p = grains_params(quality);
        p.mix = poly_volts(&[3.5]);
        p.feedback = poly_volts(&[2.0]);
        p.size = poly_volts(&[1.25]);
        let mut m = make_grains(p, SR);
        for n in 0..30_000 {
            let x = if n < 2000 {
                0.0
            } else {
                sine(330.0, n, SR, 3.0)
            };
            let trig = if n >= 2000 && n % 1500 < 10 { 5.0 } else { 0.0 };
            let (l, r) = step(&mut m, x, trig, SR);
            assert_eq!(l, r, "{quality:?}, sample {n}");
        }
    }
}

#[test]
fn a_new_fully_wet_module_passes_no_dry_signal() {
    for q in ALL_QUALITIES {
        let mut p = grains_params(q);
        p.mix = poly_volts(&[5.0]);
        let mut m = make_grains(p, SR);
        for n in 0..4800 {
            let x = ((n as f32 * 130.8 / SR).fract() * 2.0 - 1.0) * 4.0;
            let (l, r) = step(&mut m, x, 0.0, SR);
            assert!(l.abs() < 1e-4 && r.abs() < 1e-4, "{q:?} at {n}: {l} {r}");
        }
    }
}

#[test]
fn a_voice_added_by_a_patch_update_passes_no_dry_signal() {
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[5.0]);
    let mut old = make_grains(p.clone(), SR);
    for n in 0..4800 {
        step(&mut old, sine(220.0, n, SR, 4.0), 0.0, SR);
    }
    p.input = PolySignal::poly(&[Signal::Volts(0.0), Signal::Volts(0.0)]);
    let mut new = construct(p, SR);
    transfer(&mut old, &mut new);
    for n in 0..4800 {
        let x = Signal::Volts(sine(220.0, n, SR, 4.0));
        new.params.input = PolySignal::poly(&[x.clone(), x]);
        new.params.trigger = poly_volts(&[0.0]);
        new.update(SR);
        for lane in 2..4 {
            let v = new.outputs.sample.get(lane);
            assert!(v.abs() < 1e-4, "sample {n}: {v}");
        }
    }
}

#[test]
fn each_side_records_and_passes_its_own_input() {
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[0.0]);
    let mut m = construct_stereo(p, SR);
    m.on_patch_update();
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for n in 0..20_000 {
        let x = sine(440.0, n, SR, 2.0);
        // Lane 0's left and right sides.
        m.params.input = PolySignal::poly(&[Signal::Volts(x), Signal::Volts(0.0)]);
        m.update(SR);
        if n >= 10_000 {
            l.push(m.outputs.sample.get(0));
            r.push(m.outputs.sample.get(1));
        }
    }
    assert!(rms(&l) > 0.5, "left {}", rms(&l));
    assert!(rms(&r) < 1e-4 * rms(&l), "right {}", rms(&r));
}

/// Render `params` over a 440 Hz input per lane and return each lane's left
/// side while the grains sound.
fn render(mut p: GranularParams, trigger: &[f32], lanes: usize) -> Vec<Vec<f32>> {
    p.size = poly_volts(&[2.0]);
    p.shape = poly_volts(&[0.0]);
    p.time = poly_volts(&[1.0]);
    p.input = PolySignal::poly(&vec![Signal::Volts(0.0); lanes]);
    p.trigger = poly_volts(&vec![0.0; trigger.len()]);
    let mut m = make_grains(p, SR);
    let low = sides(&vec![0.0; trigger.len()]);
    let trigger = sides(trigger);
    let input = vec![Signal::Volts(0.0); lanes];
    let mut out = vec![Vec::new(); m.lane_state.len() / 2];
    for n in 0..60_000 {
        let x = sine(440.0, n, SR, 2.0);
        let mut input = input.clone();
        for s in input.iter_mut() {
            *s = Signal::Volts(x);
        }
        m.params.input = PolySignal::poly(&input);
        let level = if (48_000..48_010).contains(&n) {
            &trigger
        } else {
            &low
        };
        m.params.trigger = level.clone();
        m.update(SR);
        if n >= 48_100 {
            for (lane, o) in out.iter_mut().enumerate() {
                o.push(m.outputs.sample.get(lane * 2));
            }
        }
    }
    out
}

#[test]
fn outputs_one_stereo_lane_per_lane() {
    let mut p = grains_params(GranularQuality::Digital);
    p.freq = poly_volts(&[0.0, 0.25, 0.5]);
    let m = make_grains(p, SR);
    assert_eq!(m.lane_state.len(), 6);
    assert_eq!(m.outputs.sample.lanes(), 6);
}

#[test]
fn a_poly_trigger_only_sounds_its_own_lane() {
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[5.0]);
    let out = render(p, &[0.0, 5.0], 2);
    assert!(rms(&out[0]) < 1e-4, "untriggered lane {}", rms(&out[0]));
    assert!(rms(&out[1]) > 0.1, "triggered lane {}", rms(&out[1]));
}

#[test]
fn grain_levels_follow_their_own_voice() {
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[5.0]);
    let alone = render(p.clone(), &[5.0, 0.0], 2);
    let together = render(p, &[5.0, 5.0], 2);
    assert_eq!(alone[0], together[0]);
    assert_eq!(together[0], together[1]);
}

#[test]
fn every_voice_carries_its_dry_signal() {
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[0.0]);
    p.freq = poly_volts(&[0.0, 0.25, 0.5]);
    let out = render(p, &[0.0], 1);
    let level = rms(&out[0]);
    assert!(level > 0.5, "dry level {level}");
    assert_eq!(out[0], out[1]);
    assert_eq!(out[0], out[2]);
}

#[test]
fn dirty_tape_stays_finite_and_audible() {
    let mut p = grains_params(GranularQuality::DirtyTape);
    p.mix = poly_volts(&[3.0]);
    p.feedback = poly_volts(&[4.0]);
    let out = render(p, &[5.0, 5.0, 5.0], 3);
    for lane in out {
        assert!(lane.iter().all(|x| x.is_finite()));
        assert!(rms(&lane) > 0.1);
    }
}

#[test]
fn spread_places_each_grain_across_both_sides() {
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[5.0]);
    p.spread = poly_volts(&[5.0]);
    p.size = poly_volts(&[1.0]);
    let mut m = make_grains(p, SR);
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for n in 0..48_000 {
        let trig = if n >= 4800 && n % 2400 < 10 { 5.0 } else { 0.0 };
        let (a, b) = step(&mut m, sine(220.0, n, SR, 3.0), trig, SR);
        l.push(a);
        r.push(b);
    }
    assert!(rms(&l) > 0.05 && rms(&r) > 0.05, "{} {}", rms(&l), rms(&r));
    assert!(l.iter().zip(&r).any(|(a, b)| (a - b).abs() > 0.1));
}

/// A module whose input reads a stereo producer, so it plays both voices of
/// each lane even when `step` later feeds both sides alike.
fn construct_stereo(mut params: GranularParams, sample_rate: f32) -> Granular {
    // Read as a stereo producer's two sides, until `step` feeds them alike.
    params.input = PolySignal::mono(Signal::cable("src", "output", 0));
    params.expand_lanes(&LaneExpansion {
        channels: 2,
        producer: &|_, _| 2,
    });
    let mut outputs = GranularOutputs::default();
    outputs.set_all_lanes(2);
    let mut m = Granular {
        outputs,
        state: Default::default(),
        params,
        lane_state: (0..2).map(|_| Default::default()).collect(),
        _lane_count: 2,
        _channels: 2,
        _block_index: Default::default(),
        _connected: Default::default(),
    };
    m.init(sample_rate);
    m
}

fn records(m: &Granular, voice: usize) -> bool {
    m.lane_state[voice].voice.granular.buffer.records()
}

#[test]
fn matching_sides_play_from_one_voice_as_both_voices_would() {
    for quality in ALL_QUALITIES {
        let mut p = grains_params(quality);
        p.mix = poly_volts(&[3.5]);
        p.feedback = poly_volts(&[2.0]);
        p.size = poly_volts(&[1.25]);
        let mut one = make_grains(p.clone(), SR);
        let mut both = construct_stereo(p, SR);
        both.on_patch_update();
        assert!(one.params.mono && !both.params.mono);
        assert!(records(&one, 0) && !records(&one, 1), "{quality:?}");
        assert!(records(&both, 0) && records(&both, 1), "{quality:?}");
        // Each module seeds its own random pans, so the grains stay centred.
        let mut heard = 0.0f32;
        for n in 0..30_000 {
            let x = sine(330.0, n, SR, 3.0);
            let trig = if n % 1500 < 10 { 5.0 } else { 0.0 };
            let (l1, r1) = step(&mut one, x, trig, SR);
            let (l2, r2) = step(&mut both, x, trig, SR);
            assert!((l1 - l2).abs() < 1e-4, "{quality:?} left at {n}: {l1} {l2}");
            assert!(
                (r1 - r2).abs() < 1e-4,
                "{quality:?} right at {n}: {r1} {r2}"
            );
            heard = heard.max(l1.abs());
        }
        assert!(heard > 0.1, "{quality:?}: silent");
    }
}

#[test]
fn a_voice_that_falls_idle_gives_up_its_recording() {
    let p = grains_params(GranularQuality::Digital);
    let mut old = construct_stereo(p.clone(), SR);
    old.on_patch_update();
    record(&mut old);
    // The voice keeps its recording while it fades out, through an update
    // that lands before the fade ends, and gives it up at the first update
    // after.
    let mut narrowed = construct(p.clone(), SR);
    transfer(&mut old, &mut narrowed);
    assert!(records(&narrowed, 1));
    let mut fading = construct(p.clone(), SR);
    transfer(&mut narrowed, &mut fading);
    assert!(records(&fading, 1));
    for n in 0..4800 {
        step(&mut fading, sine(220.0, n, SR, 1.0), 0.0, SR);
    }
    let mut new = construct(p.clone(), SR);
    transfer(&mut fading, &mut new);
    assert!(recording_level(&new, 0) > 0);
    assert!(!records(&new, 1));

    // Widening again hands the idle voice a fresh recording.
    let mut wide = construct_stereo(p, SR);
    transfer(&mut new, &mut wide);
    assert!(recording_level(&wide, 0) > 0);
    assert!(records(&wide, 1));
    for n in 0..4800 {
        let (l, r) = step(&mut wide, sine(220.0, n, SR, 1.0), 0.0, SR);
        assert!(l.is_finite() && r.is_finite());
    }
}

/// Run `a` and `b` side by side over a sine and a steady trigger, asserting
/// they agree on both sides.
fn assert_play_alike(
    a: &mut Granular,
    b: &mut Granular,
    from: usize,
    samples: usize,
    sample_rate: f32,
) {
    for n in from..from + samples {
        let x = sine(330.0, n, sample_rate, 3.0);
        let trig = if n % 1500 < 10 { 5.0 } else { 0.0 };
        let (l1, r1) = step(a, x, trig, sample_rate);
        let (l2, r2) = step(b, x, trig, sample_rate);
        assert!((l1 - l2).abs() < 1e-4, "left at {n}: {l1} {l2}");
        assert!((r1 - r2).abs() < 1e-4, "right at {n}: {r1} {r2}");
    }
}

fn wet_params() -> GranularParams {
    wet_params_at(GranularQuality::Digital)
}

fn wet_params_at(quality: GranularQuality) -> GranularParams {
    let mut p = grains_params(quality);
    p.mix = poly_volts(&[3.5]);
    p.feedback = poly_volts(&[2.0]);
    p.size = poly_volts(&[1.25]);
    p
}

#[test]
fn widening_starts_the_right_voice_where_the_left_is() {
    // A module kept mono beside one widened to stereo mid-grain, just after
    // a trigger: with both sides fed alike, the widened one plays on exactly
    // as before, at every quality and through the resampler.
    for sample_rate in [SR, 44_100.0] {
        for quality in ALL_QUALITIES {
            let p = || wet_params_at(quality);
            let mut kept = make_grains(p(), sample_rate);
            let mut widened = make_grains(p(), sample_rate);
            assert_play_alike(&mut kept, &mut widened, 0, 19_503, sample_rate);
            let mut stereo = construct_stereo(p(), sample_rate);
            transfer(&mut widened, &mut stereo);
            assert_eq!(recording_level(&stereo, 1), recording_level(&stereo, 0));
            assert_play_alike(&mut kept, &mut stereo, 19_503, 10_000, sample_rate);
        }
    }
}

#[test]
fn widening_with_a_quality_change_starts_both_sides_afresh() {
    let mut old = make_grains(wet_params(), SR);
    record(&mut old);
    let mut stereo = construct_stereo(wet_params_at(GranularQuality::DirtyTape), SR);
    transfer(&mut old, &mut stereo);
    assert!(!stereo.params.mono);
    assert_eq!(recording_level(&stereo, 0), 0);
    assert_eq!(recording_level(&stereo, 1), 0);
    for n in 0..4800 {
        let (l, r) = step(&mut stereo, sine(220.0, n, SR, 1.0), 0.0, SR);
        assert!(l.is_finite() && r.is_finite());
    }
}

#[test]
fn narrowing_crossfades_the_right_side_without_a_jump() {
    // A stereo lane whose sides differ, narrowed to one voice: the right
    // side crossfades from its own grains to the left voice's, never
    // jumping further in one sample than the grains already move.
    // Fully wet: the dry signal follows the edit's new input on its own.
    let mut p = wet_params();
    p.mix = poly_volts(&[5.0]);
    let mut stereo = construct_stereo(p.clone(), SR);
    stereo.on_patch_update();
    let sides = |n: usize| {
        let (l, r) = (sine(330.0, n, SR, 3.0), sine(495.0, n, SR, 3.0));
        PolySignal::poly(&[Signal::Volts(l), Signal::Volts(r)])
    };
    let mut right = Vec::new();
    for n in 0..20_000 {
        stereo.params.input = sides(n);
        stereo.params.trigger = poly_volts(&[if n % 1500 < 10 { 5.0 } else { 0.0 }]);
        stereo.update(SR);
        right.push(stereo.outputs.sample.get(1));
    }
    let mut mono = construct(p, SR);
    transfer(&mut stereo, &mut mono);
    assert!(mono.params.mono);
    for n in 20_000..21_000 {
        let (_, r) = step(&mut mono, sine(330.0, n, SR, 3.0), 0.0, SR);
        right.push(r);
    }
    let jump = |range: std::ops::Range<usize>| {
        range
            .map(|n| (right[n] - right[n - 1]).abs())
            .fold(0.0f32, f32::max)
    };
    let before = jump(19_000..20_000);
    let across = jump(20_000..20_200);
    assert!(across <= before * 1.5 + 1e-3, "{across} vs {before}");
}

/// Play a stereo lane with its right side at `right_hz`, narrow it to one
/// voice at the left's 330 Hz, apply `then` a few samples into the
/// crossfade, and return the right side's largest one-sample jump before
/// the narrowing and across both updates. A stereo module after `then`
/// plays 330 Hz on both sides.
fn right_jumps_across_a_narrowing(
    right_hz: f32,
    then: fn(&mut Granular) -> Granular,
) -> (f32, f32) {
    let mut p = wet_params();
    p.mix = poly_volts(&[5.0]);
    let mut stereo = construct_stereo(p.clone(), SR);
    stereo.on_patch_update();
    let sides = |n: usize| {
        let (l, r) = (sine(330.0, n, SR, 3.0), sine(right_hz, n, SR, 3.0));
        PolySignal::poly(&[Signal::Volts(l), Signal::Volts(r)])
    };
    let mut right = Vec::new();
    for n in 0..20_000 {
        stereo.params.input = sides(n);
        stereo.params.trigger = poly_volts(&[if n % 1500 < 10 { 5.0 } else { 0.0 }]);
        stereo.update(SR);
        right.push(stereo.outputs.sample.get(1));
    }
    let mut mono = construct(p, SR);
    transfer(&mut stereo, &mut mono);
    for n in 20_000..20_040 {
        let (_, r) = step(&mut mono, sine(330.0, n, SR, 3.0), 0.0, SR);
        right.push(r);
    }
    let mut next = then(&mut mono);
    for n in 20_040..21_000 {
        if next.params.mono {
            right.push(step(&mut next, sine(330.0, n, SR, 3.0), 0.0, SR).1);
        } else {
            let x = sine(330.0, n, SR, 3.0);
            next.params.input = PolySignal::poly(&[Signal::Volts(x), Signal::Volts(x)]);
            next.params.trigger = poly_volts(&[0.0]);
            next.update(SR);
            right.push(next.outputs.sample.get(1));
        }
    }
    let jump = |range: std::ops::Range<usize>| {
        range
            .map(|n| (right[n] - right[n - 1]).abs())
            .fold(0.0f32, f32::max)
    };
    (jump(19_000..20_000), jump(20_000..20_300))
}

#[test]
fn an_update_during_the_narrowing_crossfade_lets_it_finish() {
    let (before, across) = right_jumps_across_a_narrowing(495.0, |mono| {
        let mut again = construct(wet_params_at_mix(5.0), SR);
        transfer(mono, &mut again);
        assert!(again.params.mono);
        again
    });
    assert!(across <= before * 1.5 + 1e-3, "{across} vs {before}");
}

#[test]
fn widening_during_the_narrowing_crossfade_carries_on_without_a_jump() {
    // The right side plays the left's pitch throughout, so the right voice
    // it starts from the left carries on what it played.
    let (before, across) = right_jumps_across_a_narrowing(330.0, |mono| {
        let mut stereo = construct_stereo(wet_params_at_mix(5.0), SR);
        transfer(mono, &mut stereo);
        assert!(!stereo.params.mono);
        stereo
    });
    assert!(across <= before * 1.5 + 1e-3, "{across} vs {before}");
}

fn wet_params_at_mix(mix: f32) -> GranularParams {
    let mut p = wet_params();
    p.mix = poly_volts(&[mix]);
    p
}

#[test]
fn widening_a_frozen_recording_keeps_it_on_both_sides() {
    let mut p = wet_params();
    p.freeze = poly_volts(&[5.0]);
    let mut m = make_grains(wet_params(), SR);
    record(&mut m);
    let mut stereo = construct_stereo(p, SR);
    transfer(&mut m, &mut stereo);
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for n in 0..24_000 {
        let trig = if n % 1500 < 10 { 5.0 } else { 0.0 };
        stereo.params.freeze = poly_volts(&[5.0]);
        let (a, b) = step(&mut stereo, 0.0, trig, SR);
        l.push(a);
        r.push(b);
    }
    assert!(rms(&l) > 0.05, "left {}", rms(&l));
    assert!(
        rms(&r) > 0.9 * rms(&l),
        "right {} left {}",
        rms(&r),
        rms(&l)
    );
}

#[test]
fn narrowing_plays_the_lane_on_from_its_left_voice() {
    let mut kept = construct_stereo(wet_params(), SR);
    kept.on_patch_update();
    let mut narrowed = construct_stereo(wet_params(), SR);
    narrowed.on_patch_update();
    assert_play_alike(&mut kept, &mut narrowed, 0, 20_000, SR);
    let mut mono = construct(wet_params(), SR);
    transfer(&mut narrowed, &mut mono);
    assert!(mono.params.mono);
    assert_play_alike(&mut kept, &mut mono, 20_000, 10_000, SR);
}

#[test]
fn a_lanes_two_voices_place_each_grain_as_one_image() {
    // With the random pans pinned, a lane playing from both voices with its
    // sides fed alike places every grain where one voice alone would: the
    // two voices share each grain's pan.
    crate::dsp::utils::rng::TEST_SEED_BASE.set(Some(0x5eed));
    let mut p = grains_params(GranularQuality::Digital);
    p.mix = poly_volts(&[3.5]);
    p.size = poly_volts(&[1.25]);
    p.spread = poly_volts(&[5.0]);
    let mut one = make_grains(p.clone(), SR);
    let mut both = construct_stereo(p, SR);
    both.on_patch_update();
    crate::dsp::utils::rng::TEST_SEED_BASE.set(None);
    let mut off_centre = 0.0f32;
    for n in 0..30_000 {
        let x = sine(330.0, n, SR, 3.0);
        let trig = if n % 1500 < 10 { 5.0 } else { 0.0 };
        let (l1, r1) = step(&mut one, x, trig, SR);
        let (l2, r2) = step(&mut both, x, trig, SR);
        assert!((l1 - l2).abs() < 1e-4 && (r1 - r2).abs() < 1e-4, "at {n}");
        off_centre = off_centre.max((l1 - r1).abs());
    }
    assert!(off_centre > 0.1, "spread places grains off centre");
}
