//! Every lane of a drum sounds the same whether it runs alone or in a
//! group, whether it runs by block or sample by sample, and keeps sounding
//! across patch updates that regroup it.

use serde_json::{Value, json};

use super::group::ALONE;
use crate::Patch;
use crate::dsp::get_params_deserializers;
use crate::dsp::utils::rng::TEST_SEED_BASE;
use crate::params::DeserializedParams;
use crate::types::ProcessingMode;

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

/// The output drum `voice` gives when read as a signal itself.
fn default_port(voice: &str) -> String {
    let outputs = super::schemas()
        .into_iter()
        .find(|s| s.name == voice)
        .unwrap()
        .outputs;
    let default = outputs.iter().position(|o| o.default).unwrap_or(0);
    outputs[default].name.clone()
}

/// Every drum voice.
fn voices() -> Vec<String> {
    super::schemas().into_iter().map(|s| s.name).collect()
}

/// How a drum's trigger and accent are patched, reaching each way a block
/// input resolves.
#[derive(Clone, Copy, Debug)]
enum Wiring {
    /// A clock cable per lane, and a constant accent per lane.
    Poly,
    /// One clock cable across every lane.
    Broadcast,
    /// An accent mixing cables and constants.
    Mixed,
}

/// A drum of `lanes` lanes struck by a clock per lane, fast enough
/// that hits run into each other, with the accent varying across lanes;
/// the hi-hat's open hat is struck by a second clock.
fn patch(voice: &str, lanes: usize, restart: bool) -> Patch {
    wired(voice, lanes, restart, Wiring::Poly, ProcessingMode::Block)
}

/// [`patch`], wired by `wiring`, with the drum running in `mode`.
fn wired(voice: &str, lanes: usize, restart: bool, wiring: Wiring, mode: ProcessingMode) -> Patch {
    let per_lane = |f: &dyn Fn(usize) -> Value| Value::Array((0..lanes).map(f).collect());
    let cable_at = |module: &str, lane: usize| json!({ "type": "cable", "module": module, "port": "output", "lane": lane });
    let cable = |module: &str| per_lane(&|lane| cable_at(module, lane));
    let accent = |lane: usize| json!([0.0, 5.0, 2.0, 3.7, 1.0, 4.4][lane % 6]);
    let trigger = if voice == "$tr808.hiHat" {
        "closed"
    } else {
        "trigger"
    };
    let mut params = json!({
        trigger: cable("clock"),
        "accent": per_lane(&accent),
        "restart": restart,
    });
    match wiring {
        Wiring::Poly => {}
        Wiring::Broadcast => params[trigger] = cable_at("clock", 0),
        Wiring::Mixed => {
            params["accent"] = per_lane(&|lane| {
                if lane % 2 == 0 {
                    cable_at("close", lane)
                } else {
                    accent(lane)
                }
            })
        }
    }
    if voice == "$tr808.hiHat" {
        params["open"] = cable("close");
    }
    let modules = [
        (
            "clock",
            "$pulse",
            json!({ "freq": per_lane(&|lane| json!(-1.3 + 0.17 * lane as f32)) }),
            ProcessingMode::Block,
        ),
        (
            "close",
            "$pulse",
            json!({ "freq": per_lane(&|lane| json!(-2.1 + 0.23 * lane as f32)) }),
            ProcessingMode::Block,
        ),
        ("drum", voice, params, mode),
    ];
    let deserializers = get_params_deserializers();
    let mut patch = Patch::new();
    patch
        .insert_modules(
            modules.into_iter().map(|(id, module_type, params, mode)| {
                let cached = deserializers[module_type](params).expect("deserialize");
                (
                    id.to_string(),
                    module_type.to_string(),
                    DeserializedParams {
                        params: cached.params,
                        lane_count: cached.lane_count,
                        f64_inputs: false,
                    },
                    mode,
                )
            }),
            SR,
            BLOCK,
        )
        .expect("insert");
    patch
}

/// Connects `patch` and runs its patch-update hooks, as a patch update does.
fn start(patch: &Patch) {
    patch.connect_all();
    for module in patch.sampleables.values() {
        module.on_patch_update();
    }
}

/// `blocks` blocks of every lane's output, appended to `out`.
fn render(patch: &Patch, blocks: usize, out: &mut [Vec<f32>]) {
    render_by(patch, blocks, &[BLOCK], out);
}

/// [`render`], each block run up to each of `steps` in turn, the drum
/// first so its reads pull its sources.
fn render_by(patch: &Patch, blocks: usize, steps: &[usize], out: &mut [Vec<f32>]) {
    let port = default_port(patch.sampleables["drum"].get_module_type());
    for _ in 0..blocks {
        for module in patch.sampleables.values() {
            module.start_block();
        }
        for &step in steps {
            for id in ["drum", "clock", "close"] {
                patch.sampleables[id].ensure_processed_to(step);
            }
        }
        for (lane, samples) in out.iter_mut().enumerate() {
            samples
                .extend((0..BLOCK).map(|i| patch.sampleables["drum"].get_value_at(&port, lane, i)));
        }
    }
}

/// `blocks` blocks of every lane of a fresh drum.
fn fresh(voice: &str, lanes: usize, restart: bool, blocks: usize) -> Vec<Vec<f32>> {
    let patch = patch(voice, lanes, restart);
    start(&patch);
    let mut out = vec![Vec::new(); lanes];
    render(&patch, blocks, &mut out);
    out
}

fn same_bits(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

#[test]
fn lanes_sound_the_same_alone_and_in_groups() {
    let voices = voices();
    TEST_SEED_BASE.set(Some(0x5eed));
    for voice in voices.iter().map(String::as_str) {
        for lanes in [2, 3, 6] {
            for restart in [false, true] {
                let grouped = fresh(voice, lanes, restart, 20);
                ALONE.set(true);
                let alone = fresh(voice, lanes, restart, 20);
                ALONE.set(false);
                for (lane, (g, a)) in grouped.iter().zip(&alone).enumerate() {
                    assert!(
                        same_bits(g, a),
                        "{voice} lane {lane} of {lanes}, restart {restart}"
                    );
                    assert!(g.iter().any(|v| *v != 0.0), "{voice} lane {lane} silent");
                }
            }
        }
    }
}

#[test]
fn block_and_sample_paths_agree() {
    let voices = voices();
    TEST_SEED_BASE.set(Some(0x5eed));
    let run = |voice, lanes, wiring, mode, steps: &[usize]| {
        let patch = wired(voice, lanes, true, wiring, mode);
        start(&patch);
        let mut out = vec![Vec::new(); lanes];
        render_by(&patch, 8, steps, &mut out);
        out
    };
    for voice in voices.iter().map(String::as_str) {
        for lanes in [1, 3, 6] {
            for wiring in [Wiring::Poly, Wiring::Broadcast, Wiring::Mixed] {
                let by_sample = run(voice, lanes, wiring, ProcessingMode::Sample, &[BLOCK]);
                for steps in [&[BLOCK][..], &[1, 10, 11, 40, BLOCK]] {
                    let by_block = run(voice, lanes, wiring, ProcessingMode::Block, steps);
                    for (lane, (b, s)) in by_block.iter().zip(&by_sample).enumerate() {
                        assert!(
                            same_bits(b, s),
                            "{voice} lane {lane} of {lanes}, {wiring:?}, steps {steps:?}"
                        );
                        assert!(b.iter().any(|v| *v != 0.0), "{voice} lane {lane} silent");
                    }
                }
            }
        }
    }
}

#[test]
fn lanes_keep_sounding_as_updates_regroup_them() {
    let voices = voices();
    TEST_SEED_BASE.set(Some(0x5eed));
    const COUNTS: [usize; 5] = [6, 4, 1, 2, 6];
    const BLOCKS: usize = 8;
    for voice in voices.iter().map(String::as_str) {
        let whole = fresh(voice, 6, false, BLOCKS * COUNTS.len());
        let mut old = patch(voice, COUNTS[0], false);
        start(&old);
        // A lane sounds on unbroken from the start until an update drops
        // it.
        let mut kept = COUNTS[0];
        for (segment, &count) in COUNTS.iter().enumerate() {
            if segment > 0 {
                let new = patch(voice, count, false);
                for (id, module) in &new.sampleables {
                    module
                        .transfer_state_from(old.sampleables[id].as_ref(), &crate::types::SameIds);
                }
                start(&new);
                old = new;
            }
            kept = kept.min(count);
            let mut out = vec![Vec::new(); count];
            render(&old, BLOCKS, &mut out);
            let span = segment * BLOCKS * BLOCK..(segment + 1) * BLOCKS * BLOCK;
            for (lane, samples) in out.iter().enumerate().take(kept) {
                assert!(
                    same_bits(samples, &whole[lane][span.clone()]),
                    "{voice} lane {lane} through {COUNTS:?}, segment {segment}"
                );
            }
        }
    }
}

mod parking {
    //! A circuit that settles within a few samples, struck on a schedule,
    //! so its elements settle, park and strike again while others ring.

    use super::super::group::{ALONE, Drum, Groups, LaneInputs, Run};
    use super::super::simd::{Mono, Real, Simd, c, elementwise};
    use super::super::voice::{Circuit, Fire, Voices};

    #[derive(Clone, Copy, Default)]
    struct Ring<L: Simd = Mono> {
        level: L::S,
        phase: L::S,
    }

    elementwise!(Ring { level, phase });

    impl Fire for Ring {
        fn fire(&mut self, accent: f32, _: &()) {
            self.level += 1.0 + accent;
        }
    }

    impl<L: Simd> Circuit<L> for Ring<L> {
        type Shared = ();
        type Controls = L::S;

        fn settled(&self) -> L::SM {
            self.level.lt(c(0.05))
        }

        fn step(&mut self, _: &(), &rate: &L::S) -> L::S {
            self.phase = (self.phase + rate).fract();
            self.level *= 0.7;
            self.level * self.phase
        }
    }

    struct RingDrum<const RUNS_IN_QUADS: bool, const RUNS_IN_PAIRS: bool>;

    impl<const Q: bool, const P: bool> Drum for RingDrum<Q, P> {
        type Shared = ();
        type Circuit<L: Simd> = Ring<L>;
        type Controls<L: Simd> = L::S;
        type Sources<L: Simd> = ();
        const RUNS_IN_QUADS: bool = Q;
        const RUNS_IN_PAIRS: bool = P;
    }

    /// Strikes lane `lane` on samples where `strikes(sample, lane)`.
    struct Script {
        sample: usize,
    }

    fn strikes(sample: usize, lane: usize) -> bool {
        (sample + 3 * lane) % (7 + 5 * lane) < 2
    }

    impl<const Q: bool, const P: bool> LaneInputs<RingDrum<Q, P>> for Script {
        type Lane = Voices<Ring>;

        fn voices(lane: &mut Voices<Ring>) -> &mut Voices<Ring> {
            lane
        }

        fn inputs(&self, lane: usize, _: &mut Voices<Ring>) -> (f32, f32) {
            let trigger = if strikes(self.sample, lane) { 5.0 } else { 0.0 };
            (trigger, 0.11 + 0.07 * lane as f32)
        }
    }

    /// Every lane's output over `samples` samples, the lane count
    /// changing to each of `counts` in turn.
    fn run<const Q: bool, const P: bool>(
        restart: bool,
        counts: &[usize],
        samples: usize,
    ) -> Vec<Vec<f32>> {
        let accent: &[f32] = &[0.0; 6];
        let run = Run::<RingDrum<Q, P>, _> {
            accent,
            restart,
            fade_samples: 3,
            shared: &(),
        };
        let mut groups = Groups::<RingDrum<Q, P>>::new();
        let mut lanes = vec![Voices::<Ring>::default(); 6];
        let mut out = vec![Vec::new(); 6];
        let mut last = 0;
        for (segment, &count) in counts.iter().enumerate() {
            // As a patch update does, lanes past the last count start
            // afresh.
            lanes[last..].fill(Voices::default());
            groups.layout::<Script>(&mut lanes, count);
            last = count;
            for sample in segment * samples..(segment + 1) * samples {
                let mut values = [0.0; 6];
                groups.run(&Script { sample }, &mut lanes[..count], &run, |lane, v| {
                    values[lane] = v;
                });
                for (lane, samples) in out.iter_mut().enumerate() {
                    samples.push(values[lane]);
                }
            }
        }
        out
    }

    fn check<const Q: bool, const P: bool>() {
        for restart in [false, true] {
            for counts in [&[6][..], &[4, 6, 3, 1, 5, 2, 6]] {
                let grouped = run::<Q, P>(restart, counts, 60);
                ALONE.set(true);
                let alone = run::<Q, P>(restart, counts, 60);
                ALONE.set(false);
                assert_eq!(
                    grouped, alone,
                    "quad {Q}, pair {P}, restart {restart}, {counts:?}"
                );
            }
        }
    }

    #[test]
    fn settled_elements_park_and_strike_again_as_alone() {
        check::<true, true>();
        check::<true, false>();
        check::<false, true>();
    }
}
