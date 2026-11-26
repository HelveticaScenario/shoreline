//! `$spectralReverb.wash`: every frequency bin holds the power it has
//! received and lets it decay, and each frame is resynthesised from the held
//! levels at fresh random phases, so the tail has no echoes or pitch of its
//! own.

use deserr::Deserr;
use rustfft::num_complex::Complex;
use schemars::JsonSchema;

use super::stft::Stft;
use super::{damped_seconds, decay_gain, decay_seconds, input_gain, shimmer_ratio_per_second};
use crate::dsp::utils::rng::{LcgRng, seed_base};
use crate::dsp::utils::{SchmittTrigger, sanitize, sin_turns};
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{LaneSum, LaneSumExt, PairSignal, PolyOutput};
use crate::reference_category::ReferenceCategory;

/// Power gain that brings a held sine back out at its input level when the
/// build-up is fully compensated: random phases add the overlapping frames
/// in power, not in amplitude.
const OUTPUT_POWER: f32 = 4.0;
/// Share of a bin's power that `blur` moves to its neighbours per frame at
/// full setting.
const MAX_BLUR_SHARE: f32 = 0.5;
/// How far `blur` reaches at full setting, as a fraction of the bin's own
/// frequency, so it smears by a musical interval at every pitch.
const MAX_BLUR_REACH: f32 = 0.02;

const DEFAULT_DECAY: f32 = 2.5;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct SpectralWashParams {
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
        "tilts the decay: positive values let the treble die first, negative the bass",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    damping: Option<LaneSum>,
    #[input(
        "smears the tail across neighbouring frequencies, dissolving its pitch into noise",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    blur: Option<LaneSum>,
    #[input(
        "slides the tail's pitch as it rings: up to an octave a second, upward when positive",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
        fold,
    )]
    #[deserr(default)]
    shimmer: Option<LaneSum>,
    #[input(
        "while high, the tail stops decaying and the input is shut out",
        type = gate,
        fold,
    )]
    #[deserr(default)]
    freeze: Option<LaneSum>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SpectralWashOutputs {
    #[output(
        "output",
        "stereo reverb, fully wet, as one stereo lane",
        type = cv,
        default
    )]
    sample: PolyOutput,
}

#[derive(Default)]
struct WashSide {
    /// Power held in each bin.
    held: Box<[f32]>,
    /// Destination of a shimmer or blur pass, swapped with `held`.
    moved: Box<[f32]>,
    rng: LcgRng,
}

/// Per-bin gains for one `decay` and `damping`.
#[derive(Default)]
struct WashTables {
    /// `(decay, damping)` the tables were built for.
    key: Option<(u32, u32)>,
    /// Share of its power a bin keeps per frame.
    kept: Box<[f32]>,
    /// Gain on the power a bin receives from the input.
    received: Box<[f32]>,
}

#[derive(Default)]
struct SpectralWashState {
    stft: Stft,
    sides: [WashSide; 2],
    tables: WashTables,
    freeze: SchmittTrigger,
    /// Seconds between two frames of one side.
    hop_seconds: f32,
    /// Width of a bin in Hz.
    bin_hz: f32,
}

#[derive(Clone, Copy)]
struct WashControls {
    decay: f32,
    damping: f32,
    blur: f32,
    shimmer: f32,
    frozen: bool,
}

impl WashTables {
    fn refresh(&mut self, controls: &WashControls, hop_seconds: f32, bin_hz: f32) {
        let key = (controls.decay.to_bits(), controls.damping.to_bits());
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        let seconds = decay_seconds(controls.decay);
        for (k, (kept, received)) in self
            .kept
            .iter_mut()
            .zip(self.received.iter_mut())
            .enumerate()
        {
            let freq = k.max(1) as f32 * bin_hz;
            let gain = decay_gain(hop_seconds, damped_seconds(seconds, freq, controls.damping));
            *kept = gain * gain;
            *received = OUTPUT_POWER * input_gain(*kept);
        }
    }
}

impl WashSide {
    /// Slide the held spectrum by `ratio` in frequency.
    fn shift(&mut self, ratio: f32) {
        let last = self.held.len() - 1;
        let step = 1.0 / ratio;
        for (k, moved) in self.moved.iter_mut().enumerate() {
            let from = k as f32 * step;
            let index = from as usize;
            *moved = if index >= last {
                0.0
            } else {
                let frac = from - index as f32;
                self.held[index] + (self.held[index + 1] - self.held[index]) * frac
            };
        }
        std::mem::swap(&mut self.held, &mut self.moved);
    }

    /// Move part of every bin's power to bins a fixed interval either side.
    fn blur(&mut self, amount: f32) {
        let last = self.held.len() - 1;
        let share = MAX_BLUR_SHARE * amount;
        let reach = MAX_BLUR_REACH * amount;
        for (k, moved) in self.moved.iter_mut().enumerate() {
            let step = 1 + (k as f32 * reach) as usize;
            let below = self.held[k.saturating_sub(step)];
            let above = self.held[(k + step).min(last)];
            *moved = self.held[k] * (1.0 - share) + (below + above) * (0.5 * share);
        }
        std::mem::swap(&mut self.held, &mut self.moved);
    }

    /// Take one frame's spectrum into the held power and replace it with the
    /// tail's.
    fn frame(
        &mut self,
        spectrum: &mut [Complex<f32>],
        tables: &WashTables,
        controls: &WashControls,
        hop_seconds: f32,
    ) {
        if controls.shimmer != 0.0 {
            self.shift(shimmer_ratio_per_second(controls.shimmer).powf(hop_seconds));
        }
        if controls.blur > 0.0 {
            self.blur(controls.blur.min(5.0) / 5.0);
        }

        let last = spectrum.len() - 1;
        spectrum[0] = Complex::default();
        spectrum[last] = Complex::default();
        self.held[0] = 0.0;
        self.held[last] = 0.0;
        for k in 1..last {
            let held = if controls.frozen {
                self.held[k]
            } else {
                sanitize(
                    self.held[k] * tables.kept[k] + spectrum[k].norm_sqr() * tables.received[k],
                )
            };
            self.held[k] = held;
            let level = held.sqrt();
            let turns = self.rng.next_unit();
            let quarter_on = if turns < 0.75 {
                turns + 0.25
            } else {
                turns - 0.75
            };
            spectrum[k] = Complex::new(level * sin_turns(quarter_on), level * sin_turns(turns));
        }
    }
}

const SPECTRAL_WASH_DOCS: ModuleDocs = ModuleDocs {
    summary: "Smooth, echo-free spectral reverb",
    description: "A reverb that holds the level of every frequency it hears and lets each fade \
                  on its own, so the tail is a seamless wash with no echoes that can ring for a \
                  minute or freeze in place. The tail swells in over about 85 ms. Output \
                  is fully wet, so blend it with `.wet(mix)` or send to it from a bus.",
    categories: &[ReferenceCategory::Effect],
    sections: &[],
    examples: &[
        DocExample {
            title: "Plucks melting into a pad",
            code: r#"
                const notes = $`c5 g4 eb5 ~`
                melody: $sine(notes)
                  .amp($perc(notes, { decay: 0.15 }))
                  .spectralReverb.wash({ decay: 4, damping: 2 })
                  .wet(3)
            "#,
        },
        DocExample {
            title: "Rising, dissolving tail",
            code: r#"
                const notes = $`c4 ~ ~ eb4 ~ ~ g4 ~`
                melody: $saw(notes)
                  .amp($perc(notes, { decay: 0.3 }))
                  .spectralReverb.wash({ decay: 3.5, blur: 2, shimmer: 1 })
                  .wet(2.5)
            "#,
        },
    ],
};

#[module(
    name = "$spectralReverb.wash", stereo = stereo,
    docs = SPECTRAL_WASH_DOCS,
    lanes = 1,
    has_init,
    args(input),
    experimental
)]
pub struct SpectralWash {
    outputs: SpectralWashOutputs,
    state: SpectralWashState,
    params: SpectralWashParams,
}

impl SpectralWash {
    /// Allocate the transform and the per-bin tables, all sized by the
    /// sample rate alone. Main thread.
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
            side.held = vec![0.0; bins].into_boxed_slice();
            side.moved = vec![0.0; bins].into_boxed_slice();
        }
        state.tables.kept = vec![0.0; bins].into_boxed_slice();
        state.tables.received = vec![0.0; bins].into_boxed_slice();
    }

    fn update(&mut self, _sample_rate: f32) {
        let input = self.params.input.sum_sides();
        let (frozen, _) = self
            .state
            .freeze
            .process_with_edge(self.params.freeze.value_or(0.0));

        let params = &self.params;
        let SpectralWashState {
            stft,
            sides,
            tables,
            hop_seconds,
            bin_hz,
            ..
        } = &mut self.state;
        for (i, (side, x)) in sides.iter_mut().zip(input).enumerate() {
            let out = stft.tick(i, sanitize(x), |spectrum| {
                let controls = WashControls {
                    decay: sanitize(params.decay.value_or(DEFAULT_DECAY)),
                    damping: sanitize(params.damping.value_or(0.0)),
                    blur: sanitize(params.blur.value_or(0.0)),
                    shimmer: sanitize(params.shimmer.value_or(0.0)),
                    frozen,
                };
                tables.refresh(&controls, *hop_seconds, *bin_hz);
                side.frame(spectrum, tables, &controls, *hop_seconds);
            });
            self.outputs.sample.set(i, out);
        }
    }
}

message_handlers!(impl SpectralWash {});

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::dsp::utils::rng::LcgRng;
    use crate::poly::{PairSignal, PolySignal};
    use crate::types::{OutputStruct, Signal};

    impl Driven for SpectralWash {
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
        damping: Option<f32>,
        blur: Option<f32>,
        shimmer: Option<f32>,
        freeze: Option<f32>,
    }

    fn wash_at(knobs: Knobs, sample_rate: f32) -> SpectralWash {
        pin_seed();
        let mut outputs = SpectralWashOutputs::default();
        outputs.set_all_lanes(2);
        let mut module = SpectralWash {
            outputs,
            state: Default::default(),
            params: SpectralWashParams {
                input: PairSignal::from_mono(PolySignal::mono(Signal::Volts(0.0))),
                decay: knobs.decay.and_then(control),
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

    fn wash(knobs: Knobs) -> SpectralWash {
        wash_at(knobs, SAMPLE_RATE)
    }

    #[test]
    fn silence_in_silence_out() {
        let (left, right) = run(&mut wash(Knobs::default()), 20_000, |_| 0.0, None);
        assert!(left.iter().chain(&right).all(|&s| s == 0.0));
    }

    #[test]
    fn the_tail_starts_within_a_hop_of_the_input() {
        let mut module = wash(Knobs::default());
        let (left, _) = run(&mut module, 20_000, tone, None);
        let first = left.iter().position(|&s| s != 0.0).unwrap();
        assert!(
            first <= module.state.stft.hop() + 1,
            "first sound at {first}"
        );
    }

    #[test]
    fn a_held_note_comes_back_near_its_level_on_both_sides() {
        let mut module = wash(Knobs {
            decay: Some(1.0),
            ..Default::default()
        });
        let (left, right) = run(&mut module, 96_000, tone, None);
        let (l, r) = (rms(&left[48_000..]), rms(&right[48_000..]));
        // A 1 V sine is 0.707 V rms, and decay 1 leaves about 2 dB of
        // build-up.
        assert!((0.6..1.2).contains(&l), "left rms {l}");
        assert!((0.6..1.2).contains(&r), "right rms {r}");
        assert!(left[48_000..] != right[48_000..], "sides should differ");
    }

    #[test]
    fn higher_decay_rings_longer() {
        let tail = |decay: f32| {
            let mut module = wash(Knobs {
                decay: Some(decay),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 96_000, burst, None);
            rms(&left[72_000..])
        };
        let (short, long) = (tail(1.0), tail(4.0));
        assert!(long > short * 10.0, "long {long}, short {short}");
    }

    #[test]
    fn damping_picks_which_end_dies_first() {
        let tail_bands = |damping: f32| {
            let mut module = wash(Knobs {
                decay: Some(3.0),
                damping: Some(damping),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 96_000, noise_burst(), None);
            band_powers(&left[64_000..])
        };
        let (flat_low, flat_high) = tail_bands(0.0);
        let (dark_low, dark_high) = tail_bands(5.0);
        let (thin_low, thin_high) = tail_bands(-5.0);
        assert!(dark_high / dark_low < 0.1 * flat_high / flat_low);
        assert!(thin_low / thin_high < 0.1 * flat_low / flat_high);
    }

    #[test]
    fn freeze_holds_the_tail_and_shuts_out_the_input() {
        let mut module = wash(Knobs {
            decay: Some(1.0),
            ..Default::default()
        });
        let (left, _) = run(&mut module, 240_000, burst, Some(24_000));
        // A note's few bins at random phases wander in level from second
        // to second; an unfrozen tail would have fallen 380 dB by `late`.
        let early = rms(&left[48_000..96_000]);
        let late = rms(&left[192_000..240_000]);
        assert!(early > 1e-3, "frozen tail should sound, got {early}");
        assert!(
            (late / early - 1.0).abs() < 0.3,
            "early {early}, late {late}"
        );

        let mut frozen = wash(Knobs {
            freeze: Some(5.0),
            ..Default::default()
        });
        let (left, _) = run(&mut frozen, 48_000, tone, None);
        assert!(left.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn shimmer_slides_the_tail_in_pitch() {
        let centroid_after = |shimmer: f32| {
            let mut module = wash(Knobs {
                decay: Some(4.0),
                shimmer: Some(shimmer),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 96_000, burst, None);
            centroid_and_spread(&left[64_000..]).0
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
            let mut module = wash(Knobs {
                decay: Some(4.0),
                blur: Some(blur),
                ..Default::default()
            });
            let (left, _) = run(&mut module, 96_000, burst, None);
            centroid_and_spread(&left[64_000..]).1
        };
        let (sharp, blurred) = (spread_after(0.0), spread_after(5.0));
        assert!(blurred > sharp * 2.0, "blurred {blurred}, sharp {sharp}");
    }

    #[test]
    fn extreme_and_non_finite_settings_stay_bounded_at_every_rate() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            for (decay, damping, blur, shimmer) in [
                (5.0, -5.0, 5.0, 5.0),
                (50.0, 50.0, 50.0, -50.0),
                (-50.0, 0.0, -3.0, 0.0),
                (f32::NAN, f32::INFINITY, f32::NAN, f32::NEG_INFINITY),
            ] {
                let mut module = wash_at(
                    Knobs {
                        decay: Some(decay),
                        damping: Some(damping),
                        blur: Some(blur),
                        shimmer: Some(shimmer),
                        freeze: None,
                    },
                    sample_rate,
                );
                let input = |n: usize| if n == 5_000 { f32::NAN } else { tone(n) };
                let (left, right) = run(&mut module, 96_000, input, None);
                let peak = peak(&left).max(peak(&right));
                assert!(
                    peak.is_finite() && peak < 20.0,
                    "{sample_rate} Hz, decay {decay}: peak {peak}"
                );
                assert!(rms(&left[48_000..]) > 0.0, "the tail survives a NaN input");
            }
        }
    }

    #[test]
    fn held_notes_settle_at_the_longest_decay() {
        for (blur, shimmer) in [(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (5.0, -5.0)] {
            let mut module = wash(Knobs {
                decay: Some(5.0),
                blur: Some(blur),
                shimmer: Some(shimmer),
                ..Default::default()
            });
            let (left, right) = run(&mut module, 1_920_000, tone, None);
            for side in [&left, &right] {
                let (settled, late) = (rms(&side[960_000..1_440_000]), rms(&side[1_440_000..]));
                assert!(
                    late < settled * 1.3 && peak(side) < 50.0,
                    "blur {blur}, shimmer {shimmer}: settled {settled}, late {late}, peak {}",
                    peak(side)
                );
            }
        }
    }

    #[test]
    fn a_frozen_blurred_tail_never_grows() {
        for shimmer in [-5.0, 0.0, 5.0] {
            let mut module = wash(Knobs {
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
        let mut module = wash(Knobs::default());
        let mut rng = LcgRng::default();
        rng.seed(11, 0);
        let mut out = Vec::with_capacity(1_920_000);
        for n in 0..960_000 {
            if n % 1_024 == 0 {
                let params = &mut module.params;
                params.decay = control(rng.next_unit() * 5.0);
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
}
