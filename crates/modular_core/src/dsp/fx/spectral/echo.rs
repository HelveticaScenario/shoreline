//! `$spectralReverb.echo`: every frequency bin feeds back through its own
//! delay of whole frames with its phase kept, so the tail is a cloud of
//! echoes that repeat at a different time for each pitch.

use deserr::Deserr;
use rustfft::num_complex::Complex;
use schemars::JsonSchema;

use super::stft::Stft;
use super::{damped_seconds, decay_gain, decay_seconds, input_gain, shimmer_ratio_per_second};
use crate::dsp::utils::rng::{LcgRng, hash_bipolar, seed_base};
use crate::dsp::utils::{SchmittTrigger, sanitize, sin_turns};
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{LaneSum, LaneSumExt, PairSignal, PolyOutput};
use crate::reference_category::ReferenceCategory;

/// Frames of spectrum each side keeps: covers [`MAX_SPAN_SECONDS`] at the
/// shortest hop any engine rate gives.
const HISTORY_FRAMES: usize = 72;
/// Shortest delay, in frames: one whole frame length, so a frame never feeds
/// the frames it overlaps.
const MIN_DELAY_FRAMES: usize = 4;
/// Longest bin delay at either end of `size`.
const MIN_SPAN_SECONDS: f32 = 0.1;
const MAX_SPAN_SECONDS: f32 = 1.4;
/// Keeps every unfrozen loop lossy.
const MAX_FEEDBACK: f32 = 0.9999;
/// Seeds the per-bin delay scatter, the same in every instance.
const SCATTER_SEED: u64 = 0x5CA7_7E12;
/// Marks a bin whose shimmer source lies outside the spectrum.
const NO_SOURCE: u16 = u16::MAX;
/// Share of a bin's power that `blur` takes from two bins either side of it
/// per pass at full setting.
const MAX_BLUR_SHARE: f32 = 0.5;
/// How far away those bins are at full setting, as a fraction of the bin's
/// own frequency: about a semitone.
const MAX_BLUR_REACH: f32 = 0.06;
/// Largest random phase turn `blur` gives a bin per pass, either way.
const MAX_BLUR_TURNS: f32 = 0.5;

const DEFAULT_DECAY: f32 = 2.5;
const DEFAULT_SIZE: f32 = 2.5;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SpectralEchoParams {
    #[input(
        "signal to reverberate: every lane's left side feeds the left and its right side the right, and a mono lane feeds both",
        type = audio,
    )]
    input: PairSignal,
    #[input(
        "how long the tail rings: 0 about 0.2 s, 2.5 about 3.5 s, 5 about a minute",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    decay: Option<LaneSum>,
    #[input(
        "how far apart the echoes fall: the slowest pitches repeat every 0.1 s at 0, every 1.4 s at 5",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    size: Option<LaneSum>,
    #[input(
        "tilts the decay: positive values let the treble die first, negative the bass",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    damping: Option<LaneSum>,
    #[input(
        "scrambles each echo a little more than the last, softening the repeats into a haze",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    blur: Option<LaneSum>,
    #[input(
        "steps each echo's pitch from the last: up to an octave a second, upward when positive",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    shimmer: Option<LaneSum>,
    #[input(
        "while high, the echoes stop decaying and the input is shut out",
        type = gate,
        fold,
    )]
    #[deserr(default)]
    freeze: Option<LaneSum>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SpectralEchoOutputs {
    #[output(
        "output",
        "stereo reverb, fully wet, as one stereo lane",
        type = cv,
        default
    )]
    sample: PolyOutput,
}

/// One side's per-bin loop settings for one set of controls.
#[derive(Default)]
struct EchoTables {
    /// Delay in frames.
    delay: Box<[u8]>,
    /// Bin the feedback is read from, or [`NO_SOURCE`].
    source: Box<[u16]>,
    /// Feedback gain per pass.
    feedback: Box<[f32]>,
    /// Gain on the input entering the loop.
    send: Box<[f32]>,
}

#[derive(Default)]
struct EchoSide {
    /// [`HISTORY_FRAMES`] rows of one spectrum each; `row` is written next.
    history: Box<[Complex<f32>]>,
    row: usize,
    rng: LcgRng,
    tables: EchoTables,
    /// `(decay, size, damping, shimmer)` the tables were built for.
    key: Option<[u32; 4]>,
}

#[derive(Default)]
struct SpectralEchoState {
    stft: Stft,
    sides: [EchoSide; 2],
    freeze: SchmittTrigger,
    /// Seconds between two frames of one side.
    hop_seconds: f32,
    /// Width of a bin in Hz.
    bin_hz: f32,
}

#[derive(Clone, Copy)]
struct EchoControls {
    decay: f32,
    size: f32,
    damping: f32,
    blur: f32,
    shimmer: f32,
    frozen: bool,
}

impl EchoSide {
    fn refresh(&mut self, index: usize, controls: &EchoControls, hop_seconds: f32, bin_hz: f32) {
        let key = [
            controls.decay.to_bits(),
            controls.size.to_bits(),
            controls.damping.to_bits(),
            controls.shimmer.to_bits(),
        ];
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);

        let seconds = decay_seconds(controls.decay);
        let span_seconds = MIN_SPAN_SECONDS
            * (MAX_SPAN_SECONDS / MIN_SPAN_SECONDS).powf(controls.size.clamp(0.0, 5.0) / 5.0);
        let longest = (span_seconds / hop_seconds)
            .clamp(MIN_DELAY_FRAMES as f32, (HISTORY_FRAMES - 1) as f32);
        let ratio_per_second = shimmer_ratio_per_second(controls.shimmer);
        let last = self.tables.delay.len() - 1;
        let tables = &mut self.tables;
        for k in 0..=last {
            let scatter = hash_bipolar(SCATTER_SEED + index as u64, k as u64) as f32 * 0.5 + 0.5;
            let frames =
                (MIN_DELAY_FRAMES as f32 + scatter * (longest - MIN_DELAY_FRAMES as f32)).round();
            let pass_seconds = frames * hop_seconds;
            let t60 = damped_seconds(seconds, k.max(1) as f32 * bin_hz, controls.damping);
            let feedback = decay_gain(pass_seconds, t60).min(MAX_FEEDBACK);
            let source = (k as f32 / ratio_per_second.powf(pass_seconds)).round();
            tables.delay[k] = frames as u8;
            tables.feedback[k] = feedback;
            tables.send[k] = input_gain(feedback * feedback).sqrt();
            tables.source[k] = if source >= 1.0 && source < last as f32 {
                source as u16
            } else {
                NO_SOURCE
            };
        }
    }

    /// Feed one frame's spectrum into the loops and replace it with the
    /// echoes due this frame.
    fn frame(&mut self, spectrum: &mut [Complex<f32>], controls: &EchoControls) {
        let bins = spectrum.len();
        let last = bins - 1;
        let blur = controls.blur.clamp(0.0, 5.0) / 5.0;
        let share = MAX_BLUR_SHARE * blur;
        let (own, lent) = ((1.0 - share).sqrt(), (0.5 * share).sqrt());
        let write = self.row * bins;
        spectrum[0] = Complex::default();
        spectrum[last] = Complex::default();
        for k in 1..last {
            let tables = &self.tables;
            let source = tables.source[k];
            let mut fed = Complex::default();
            if source != NO_SOURCE {
                let delay = tables.delay[k] as usize;
                let row = (self.row + HISTORY_FRAMES - delay) % HISTORY_FRAMES;
                let at = row * bins + source as usize;
                fed = self.history[at];
                if blur > 0.0 {
                    let source = source as usize;
                    let step = 1 + (source as f32 * MAX_BLUR_REACH * blur) as usize;
                    let below = row * bins + source.saturating_sub(step);
                    let above = row * bins + (source + step).min(last);
                    let (below, above) = (self.history[below], self.history[above]);
                    // The mix sets the phase; the level is the three bins'
                    // power at these weights, which never exceeds the
                    // loudest of them, so in-step neighbours cannot add up
                    // past the feedback gain.
                    let mixed = fed * own + (below + above) * lent;
                    let power = (1.0 - share) * fed.norm_sqr()
                        + 0.5 * share * (below.norm_sqr() + above.norm_sqr());
                    let mixed_power = mixed.norm_sqr();
                    if mixed_power > f32::MIN_POSITIVE {
                        fed = mixed * (power / mixed_power).sqrt();
                    }
                    let turns = (self.rng.next_bipolar() * MAX_BLUR_TURNS * blur).rem_euclid(1.0);
                    let quarter_on = if turns < 0.75 {
                        turns + 0.25
                    } else {
                        turns - 0.75
                    };
                    fed *= Complex::new(sin_turns(quarter_on), sin_turns(turns));
                }
            }
            let kept = if controls.frozen {
                fed
            } else {
                spectrum[k] * tables.send[k] + fed * tables.feedback[k]
            };
            self.history[write + k] = Complex::new(sanitize(kept.re), sanitize(kept.im));
            spectrum[k] = fed;
        }
        self.row = (self.row + 1) % HISTORY_FRAMES;
    }
}

const SPECTRAL_ECHO_DOCS: ModuleDocs = ModuleDocs {
    summary: "Spectral reverb of scattered echoes",
    description: "A reverb where every frequency repeats at its own pace, so a note comes back \
                  as a shimmering scatter of echoes that thickens into a resonant tail, and can \
                  freeze in place. No echo arrives sooner than a tenth of a second after the \
                  input. Output is fully wet, so blend it with `.wet(mix)` or send to it from a bus.",
    categories: &[ReferenceCategory::Effect],
    sections: &[],
    examples: &[
        DocExample {
            title: "Plucks scattering into echoes",
            code: r#"
                const notes = $`c5 g4 eb5 ~`
                melody: $sine(notes)
                  .amp($perc(notes, { decay: 0.15 }))
                  .spectralReverb.echo({ decay: 4, size: 3.5 }).wet(3)
            "#,
        },
        DocExample {
            title: "Climbing, hazy repeats",
            code: r#"
                const notes = $`c4 ~ ~ eb4 ~ ~ g4 ~`
                melody: $saw(notes)
                  .amp($perc(notes, { decay: 0.3 }))
                  .spectralReverb.echo({
                    decay: 3.5,
                    blur: 2,
                    shimmer: 2,
                  }).wet(2.5)
            "#,
        },
    ],
};

#[module(
    name = "$spectralReverb.echo", stereo = stereo,
    docs = SPECTRAL_ECHO_DOCS,
    lanes = 1,
    has_init,
    args(input),
    experimental
)]
pub struct SpectralEcho {
    outputs: SpectralEchoOutputs,
    state: SpectralEchoState,
    params: SpectralEchoParams,
}

impl SpectralEcho {
    /// Allocate the transform, the spectrum histories and the per-bin
    /// tables, all sized by the sample rate alone. Main thread.
    fn init(&mut self, sample_rate: f32) {
        // Seeded once: a patch update swaps the running streams into the
        // rebuilt module, so the phases never repeat.
        let seed = seed_base(self);
        let state = &mut self.state;
        state.stft.allocate(sample_rate);
        let bins = state.stft.bins();
        state.hop_seconds = state.stft.hop() as f32 / sample_rate;
        state.bin_hz = sample_rate / state.stft.size() as f32;
        for (i, side) in state.sides.iter_mut().enumerate() {
            side.rng.seed(seed, i);
            side.history = vec![Complex::default(); bins * HISTORY_FRAMES].into_boxed_slice();
            side.tables = EchoTables {
                delay: vec![0; bins].into_boxed_slice(),
                source: vec![NO_SOURCE; bins].into_boxed_slice(),
                feedback: vec![0.0; bins].into_boxed_slice(),
                send: vec![0.0; bins].into_boxed_slice(),
            };
        }
    }

    fn update(&mut self, _sample_rate: f32) {
        let input = self.params.input.sum_sides();
        let (frozen, _) = self
            .state
            .freeze
            .process_with_edge(self.params.freeze.value_or(0.0));

        let params = &self.params;
        let SpectralEchoState {
            stft,
            sides,
            hop_seconds,
            bin_hz,
            ..
        } = &mut self.state;
        for (i, (side, x)) in sides.iter_mut().zip(input).enumerate() {
            let out = stft.tick(i, sanitize(x), |spectrum| {
                let controls = EchoControls {
                    decay: sanitize(params.decay.value_or(DEFAULT_DECAY)),
                    size: sanitize(params.size.value_or(DEFAULT_SIZE)),
                    damping: sanitize(params.damping.value_or(0.0)),
                    blur: sanitize(params.blur.value_or(0.0)),
                    shimmer: sanitize(params.shimmer.value_or(0.0)),
                    frozen,
                };
                side.refresh(i, &controls, *hop_seconds, *bin_hz);
                side.frame(spectrum, &controls);
            });
            self.outputs.sample.set(i, out);
        }
    }
}

message_handlers!(impl SpectralEcho {});

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::poly::{PairSignal, PolySignal};
    use crate::types::{OutputStruct, Signal};

    impl Driven for SpectralEcho {
        fn set_input(&mut self, input: PairSignal) {
            self.params.input = input;
        }
        fn set_freeze(&mut self, freeze: Option<LaneSum>) {
            self.params.freeze = freeze;
        }
        fn step(&mut self) -> [f32; 2] {
            self.update(0.0);
            [self.outputs.sample.get(0), self.outputs.sample.get(1)]
        }
    }

    #[derive(Default)]
    struct Knobs {
        decay: Option<f32>,
        size: Option<f32>,
        damping: Option<f32>,
        blur: Option<f32>,
        shimmer: Option<f32>,
        freeze: Option<f32>,
    }

    fn echo_at(knobs: Knobs, sample_rate: f32) -> SpectralEcho {
        pin_seed();
        let mut outputs = SpectralEchoOutputs::default();
        outputs.set_all_lanes(2);
        let mut module = SpectralEcho {
            outputs,
            state: Default::default(),
            params: SpectralEchoParams {
                input: PairSignal::from_mono(PolySignal::mono(Signal::Volts(0.0))),
                decay: knobs.decay.and_then(control),
                size: knobs.size.and_then(control),
                damping: knobs.damping.and_then(control),
                blur: knobs.blur.and_then(control),
                shimmer: knobs.shimmer.and_then(control),
                freeze: knobs.freeze.and_then(control),
            },
            _lane_count: 2,
            _channels: 2,
            _block_index: Default::default(),
            _connected: Default::default(),
        };
        module.init(sample_rate);
        module
    }

    fn echo(knobs: Knobs) -> SpectralEcho {
        echo_at(knobs, SAMPLE_RATE)
    }

    #[test]
    fn silence_in_silence_out() {
        let (left, right) = run(&mut echo(Knobs::default()), 40_000, |_| 0.0, None);
        assert!(left.iter().chain(&right).all(|&s| s == 0.0));
    }

    #[test]
    fn nothing_sounds_before_the_shortest_delay() {
        let mut module = echo(Knobs::default());
        let (left, _) = run(&mut module, 40_000, tone, None);
        let first = left.iter().position(|&s| s != 0.0).unwrap();
        let hop = module.state.stft.hop();
        assert!(
            first > (MIN_DELAY_FRAMES + 1) * hop,
            "first sound at {first}"
        );
        assert!(rms(&left[20_000..]) > 0.05, "the echoes arrive");
    }

    #[test]
    fn a_held_note_comes_back_near_its_level_on_both_sides() {
        let mut module = echo(Knobs {
            decay: Some(1.0),
            ..Default::default()
        });
        let (left, right) = run(&mut module, 144_000, tone, None);
        let (l, r) = (rms(&left[96_000..]), rms(&right[96_000..]));
        assert!((0.3..2.0).contains(&l), "left rms {l}");
        assert!((0.3..2.0).contains(&r), "right rms {r}");
        assert!(left[96_000..] != right[96_000..], "sides should differ");
    }

    #[test]
    fn higher_decay_rings_longer() {
        let tail = |decay: f32| {
            let mut module = echo(Knobs {
                decay: Some(decay),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 144_000, burst, None);
            rms(&left[96_000..])
        };
        let (short, long) = (tail(1.0), tail(4.0));
        assert!(long > short * 10.0, "long {long}, short {short}");
    }

    #[test]
    fn size_spaces_the_echoes() {
        // A burst shorter than the gap leaves silence before its first
        // repeat; the gap grows with size.
        let quiet_until = |size: f32| {
            let mut module = echo(Knobs {
                size: Some(size),
                ..Default::default()
            });
            let (left, _) = run(
                &mut module,
                96_000,
                |n| if n < 2_000 { tone(n) } else { 0.0 },
                None,
            );
            let total: f32 = left.iter().map(|s| s * s).sum();
            let mut seen = 0.0;
            left.iter()
                .position(|s| {
                    seen += s * s;
                    seen > total * 0.5
                })
                .unwrap()
        };
        let (small, large) = (quiet_until(0.0), quiet_until(5.0));
        assert!(large > small * 2, "large {large}, small {small}");
    }

    #[test]
    fn damping_picks_which_end_dies_first() {
        let tail_bands = |damping: f32| {
            let mut module = echo(Knobs {
                decay: Some(3.0),
                damping: Some(damping),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 144_000, noise_burst(), None);
            band_powers(&left[96_000..])
        };
        let (flat_low, flat_high) = tail_bands(0.0);
        let (dark_low, dark_high) = tail_bands(5.0);
        let (thin_low, thin_high) = tail_bands(-5.0);
        assert!(dark_high / dark_low < 0.1 * flat_high / flat_low);
        assert!(thin_low / thin_high < 0.1 * flat_low / flat_high);
    }

    #[test]
    fn freeze_holds_the_tail_and_shuts_out_the_input() {
        let mut module = echo(Knobs {
            decay: Some(1.0),
            size: Some(0.0),
            ..Default::default()
        });
        let (left, _) = run(&mut module, 240_000, burst, Some(24_000));
        let early = rms(&left[48_000..96_000]);
        let late = rms(&left[192_000..240_000]);
        assert!(early > 1e-3, "frozen tail should sound, got {early}");
        assert!(
            (late / early - 1.0).abs() < 0.3,
            "early {early}, late {late}"
        );

        let mut frozen = echo(Knobs {
            freeze: Some(5.0),
            ..Default::default()
        });
        let (left, _) = run(&mut frozen, 48_000, tone, None);
        assert!(left.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn shimmer_steps_the_echoes_in_pitch() {
        let centroid_after = |shimmer: f32| {
            let mut module = echo(Knobs {
                decay: Some(4.0),
                shimmer: Some(shimmer),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 144_000, burst, None);
            centroid_and_spread(&left[96_000..]).0
        };
        let (down, still, up) = (
            centroid_after(-3.0),
            centroid_after(0.0),
            centroid_after(3.0),
        );
        assert!(up > still * 1.3, "up {up}, still {still}");
        assert!(down < still / 1.3, "down {down}, still {still}");
    }

    #[test]
    fn blur_widens_the_tail() {
        let spread_after = |blur: f32| {
            let mut module = echo(Knobs {
                decay: Some(4.0),
                blur: Some(blur),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 144_000, burst, None);
            centroid_and_spread(&left[96_000..]).1
        };
        let (sharp, blurred) = (spread_after(0.0), spread_after(5.0));
        assert!(blurred > sharp * 1.5, "blurred {blurred}, sharp {sharp}");
    }

    #[test]
    fn blurred_and_shimmering_tails_still_decay() {
        for (size, blur, shimmer) in [
            (0.39, 0.86, -0.49),
            (0.0, 5.0, 0.0),
            (0.0, 2.0, 3.0),
            (5.0, 5.0, -5.0),
        ] {
            let mut module = echo(Knobs {
                decay: Some(3.46),
                size: Some(size),
                blur: Some(blur),
                shimmer: Some(shimmer),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 480_000, burst, None);
            let (early, late) = (rms(&left[48_000..96_000]), rms(&left[432_000..]));
            assert!(
                late < early,
                "size {size}, blur {blur}, shimmer {shimmer}: early {early}, late {late}"
            );
        }
    }

    #[test]
    fn held_notes_settle_at_the_longest_decay() {
        // A minute-long decay settles within about 20 s; after that the
        // level only wanders with the blur's random phase.
        for (size, blur, shimmer, freq) in [
            (0.0, 0.0, 0.0, 110.0),
            (0.0, 5.0, 0.0, 110.0),
            (0.0, 5.0, 5.0, 437.3),
            (5.0, 5.0, -5.0, 2_011.0),
        ] {
            let mut module = echo(Knobs {
                decay: Some(5.0),
                size: Some(size),
                blur: Some(blur),
                shimmer: Some(shimmer),
                ..Default::default()
            });
            let sine = |n: usize| (std::f32::consts::TAU * freq * n as f32 / SAMPLE_RATE).sin();
            let (left, right) = run(&mut module, 1_920_000, sine, None);
            for side in [&left, &right] {
                let (settled, late) = (rms(&side[960_000..1_440_000]), rms(&side[1_440_000..]));
                assert!(
                    late < settled * 1.3 && peak(side) < 50.0,
                    "{freq} Hz, size {size}, blur {blur}, shimmer {shimmer}: \
                     settled {settled}, late {late}, peak {}",
                    peak(side)
                );
            }
        }
    }

    #[test]
    fn a_frozen_blurred_tail_never_grows() {
        for shimmer in [-5.0, 0.0, 5.0] {
            let mut module = echo(Knobs {
                size: Some(0.0),
                blur: Some(5.0),
                shimmer: Some(shimmer),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 1_440_000, noise_burst(), Some(BURST + 4_096));
            let (early, late) = (rms(&left[48_000..144_000]), rms(&left[1_344_000..]));
            assert!(
                late <= early * 1.01,
                "shimmer {shimmer}: early {early}, late {late}"
            );
        }
    }

    #[test]
    fn controls_swept_every_frame_stay_bounded() {
        let mut module = echo(Knobs::default());
        let mut rng = LcgRng::default();
        rng.seed(11, 0);
        let mut out = Vec::with_capacity(960_000);
        for n in 0..960_000 {
            if n % 1_024 == 0 {
                let params = &mut module.params;
                params.decay = control(rng.next_unit() * 5.0);
                params.size = control(rng.next_unit() * 5.0);
                params.damping = control(rng.next_bipolar() * 5.0);
                params.blur = control(rng.next_unit() * 5.0);
                params.shimmer = control(rng.next_bipolar() * 5.0);
                params.freeze = control(if rng.next_unit() < 0.2 { 5.0 } else { 0.0 });
            }
            module.set_input(PairSignal::from_mono(PolySignal::mono(Signal::Volts(
                tone(n),
            ))));
            out.extend(module.step());
        }
        assert!(peak(&out) < 50.0, "peak {}", peak(&out));
    }

    #[test]
    fn extreme_and_non_finite_settings_stay_bounded_at_every_rate() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            for (decay, size, damping, blur, shimmer) in [
                (5.0, 5.0, -5.0, 5.0, 5.0),
                (50.0, 50.0, 50.0, 50.0, -50.0),
                (-50.0, -50.0, 0.0, -3.0, 0.0),
                (
                    f32::NAN,
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NAN,
                    f32::NEG_INFINITY,
                ),
            ] {
                let mut module = echo_at(
                    Knobs {
                        decay: Some(decay),
                        size: Some(size),
                        damping: Some(damping),
                        blur: Some(blur),
                        shimmer: Some(shimmer),
                        freeze: None,
                    },
                    sample_rate,
                );
                let input = |n: usize| if n == 5_000 { f32::NAN } else { tone(n) };
                let (left, right) = run(&mut module, 144_000, input, None);
                let peak = peak(&left).max(peak(&right));
                assert!(
                    peak.is_finite() && peak < 20.0,
                    "{sample_rate} Hz, decay {decay}: peak {peak}"
                );
                assert!(rms(&left[96_000..]) > 0.0, "the tail survives a NaN input");
            }
        }
    }
}
