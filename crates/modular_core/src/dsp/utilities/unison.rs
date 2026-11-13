use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PORT_MAX_LANES, PolyOutput, PolySignal, PolySignalExt};

fn default_count() -> usize {
    1
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct UnisonParams {
    #[input("pitch to fan out into detuned copies", type = pitch)]
    input: PolySignal,
    #[input("number of detuned copies per input lane", range = (1, 64))]
    #[serde(default = "default_count")]
    #[deserr(default = default_count())]
    count: usize,
    #[input(
        "detune width, finer near 0: 0 none, 10 puts the outer copies an octave above and below",
        type = control,
        range = (0.0, 10.0),
    )]
    #[deserr(default)]
    spread: Option<PolySignal>,
}

/// Lanes of the widest of `input`/`spread`. The two cycle against each other,
/// so the narrower one repeats across the wider one's lanes.
///
/// `update` drives its loop from this and the lane count derives from it, so
/// the two agree: every lane the module declares is a lane it writes.
fn unison_poly_lanes(input: &PolySignal, spread: &Option<PolySignal>) -> usize {
    input.lanes().max(spread.lane_count()).max(1)
}

/// Custom lane count: max(input, spread) lanes * count, clamped to 64.
#[allow(private_interfaces)]
pub fn unison_derive_lane_count(params: &UnisonParams) -> usize {
    let poly_lanes = unison_poly_lanes(&params.input, &params.spread);
    let count = params.count.clamp(1, PORT_MAX_LANES);
    (poly_lanes * count).min(PORT_MAX_LANES)
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct UnisonOutputs {
    #[output(
        "output",
        "the detuned copies, each a full set of the input lanes",
        type = cv,
        default
    )]
    sample: PolyOutput,
}

const UNISON_DOCS: ModuleDocs = ModuleDocs {
    summary: "Detuned unison copies",
    description: "Turns each pitch into a stack of detuned copies for thick unison \
                  leads and supersaw-style pads. Each copy is a full set of the \
                  input's lanes, up to 64 lanes in all.",
    categories: &[ModuleCategory::Pitch],
    sections: &[],
    examples: &[
        DocExample {
            title: "Thick seven-voice saw",
            code: r#"
                $saw($unison('c3', 7, 3))
                  .lpf('c6', 1)
                  .out()
            "#,
        },
        DocExample {
            title: "Chord with breathing detune",
            code: r#"
                $saw(
                  $unison($chord('C3[m9]'), 3, $sine('0.1hz').range(0, 4)),
                ).out()
            "#,
        },
    ],
};

#[module(name = "$unison", stereo = mono, docs = UNISON_DOCS, lanes_derive = unison_derive_lane_count, args(input, count, spread))]
pub struct Unison {
    outputs: UnisonOutputs,
    params: UnisonParams,
}

impl Unison {
    fn update(&mut self, _sample_rate: f32) {
        let count = self.params.count.clamp(1, PORT_MAX_LANES);
        let input = &self.params.input;
        let spread = &self.params.spread;

        let poly_lanes = unison_poly_lanes(input, spread);
        let output_lanes = self.lane_count();

        for poly_ch in 0..poly_lanes {
            // Both reads cycle, so whichever of the two is narrower repeats
            // across the wider one's lanes.
            let input_val = input.get_value(poly_ch);
            let spread_v = spread.value_or(poly_ch, 0.0).clamp(0.0, 10.0);
            // Exponential mapping: (spread_v / 10)^2 gives 0–1 V/Oct (0–1 octave)
            let normalized = spread_v / 10.0;
            let max_detune_voct = normalized * normalized;

            for voice in 0..count {
                let out_ch = voice * poly_lanes + poly_ch;
                if out_ch >= output_lanes {
                    break;
                }

                let offset = if count > 1 {
                    // Symmetric fan-out: -max_detune to +max_detune
                    max_detune_voct * (2.0 * voice as f32 / (count - 1) as f32 - 1.0)
                } else {
                    0.0
                };

                self.outputs.sample.set(out_ch, input_val + offset);
            }
        }
    }
}

message_handlers!(impl Unison {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputStruct, Signal};

    /// Create a Unison with params and properly initialize _lane_count and output lanes.
    fn make_unison(params: UnisonParams) -> Unison {
        let lanes = unison_derive_lane_count(&params);
        let mut outputs = UnisonOutputs::default();
        outputs.set_all_lanes(lanes);
        Unison {
            params,
            outputs,
            _lane_count: lanes,
            _channels: 1,
            _block_index: Default::default(),
        }
    }

    #[test]
    fn test_passthrough_count_1() {
        let mut u = make_unison(UnisonParams {
            input: PolySignal::mono(Signal::Volts(1.0)),
            count: 1,
            spread: Some(PolySignal::mono(Signal::Volts(5.0))),
        });
        u.update(48000.0);
        assert_eq!(u.outputs.sample.lanes(), 1);
        assert_eq!(u.outputs.sample.get(0), 1.0);
    }

    #[test]
    fn test_no_spread_duplicates() {
        // count=3, spread=0 -> 3 identical copies
        let mut u = make_unison(UnisonParams {
            input: PolySignal::mono(Signal::Volts(2.0)),
            count: 3,
            spread: None,
        });
        u.update(48000.0);
        assert_eq!(u.outputs.sample.lanes(), 3);
        assert_eq!(u.outputs.sample.get(0), 2.0);
        assert_eq!(u.outputs.sample.get(1), 2.0);
        assert_eq!(u.outputs.sample.get(2), 2.0);
    }

    #[test]
    fn test_symmetric_spread() {
        // count=3, spread=10V -> max_detune = 1.0 V/Oct
        // voice 0: -1.0, voice 1: 0.0, voice 2: +1.0
        let mut u = make_unison(UnisonParams {
            input: PolySignal::mono(Signal::Volts(0.0)),
            count: 3,
            spread: Some(PolySignal::mono(Signal::Volts(10.0))),
        });
        u.update(48000.0);
        assert_eq!(u.outputs.sample.lanes(), 3);
        assert!((u.outputs.sample.get(0) - (-1.0)).abs() < 1e-6);
        assert!((u.outputs.sample.get(1) - 0.0).abs() < 1e-6);
        assert!((u.outputs.sample.get(2) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_exponential_curve() {
        // spread=5V -> (5/10)^2 = 0.25 V/Oct
        let mut u = make_unison(UnisonParams {
            input: PolySignal::mono(Signal::Volts(0.0)),
            count: 3,
            spread: Some(PolySignal::mono(Signal::Volts(5.0))),
        });
        u.update(48000.0);
        assert!((u.outputs.sample.get(0) - (-0.25)).abs() < 1e-6);
        assert!((u.outputs.sample.get(1) - 0.0).abs() < 1e-6);
        assert!((u.outputs.sample.get(2) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_poly_input_expansion() {
        // 2-lane input, count=3 -> 6 output lanes
        let mut u = make_unison(UnisonParams {
            input: PolySignal::poly(&[Signal::Volts(0.0), Signal::Volts(1.0)]),
            count: 3,
            spread: Some(PolySignal::mono(Signal::Volts(10.0))),
        });
        u.update(48000.0);
        assert_eq!(u.outputs.sample.lanes(), 6);
        // Voices are interleaved: each voice spans both input lanes.
        // Voice 0 (-1.0 detune): input lane 0, input lane 1
        assert!((u.outputs.sample.get(0) - (-1.0)).abs() < 1e-6);
        assert!((u.outputs.sample.get(1) - 0.0).abs() < 1e-6);
        // Voice 1 (no detune)
        assert!((u.outputs.sample.get(2) - 0.0).abs() < 1e-6);
        assert!((u.outputs.sample.get(3) - 1.0).abs() < 1e-6);
        // Voice 2 (+1.0 detune)
        assert!((u.outputs.sample.get(4) - 1.0).abs() < 1e-6);
        assert!((u.outputs.sample.get(5) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_clamp_to_max_lanes() {
        // 16-lane input, count=5 -> 80 desired, clamped to PORT_MAX_LANES
        let input: Vec<Signal> = (0..16).map(|i| Signal::Volts(i as f32)).collect();
        let mut u = make_unison(UnisonParams {
            input: PolySignal::poly(&input),
            count: 5,
            spread: None,
        });
        u.update(48000.0);
        assert_eq!(u.outputs.sample.lanes(), PORT_MAX_LANES);
    }

    #[test]
    fn test_lane_count_derivation() {
        // 1 lane * 7 = 7
        let params = UnisonParams {
            input: PolySignal::mono(Signal::Volts(0.0)),
            count: 7,
            spread: None,
        };
        assert_eq!(unison_derive_lane_count(&params), 7);

        // 3 lanes * 5 = 15
        let params = UnisonParams {
            input: PolySignal::poly(&[Signal::Volts(0.0), Signal::Volts(0.0), Signal::Volts(0.0)]),
            count: 5,
            spread: None,
        };
        assert_eq!(unison_derive_lane_count(&params), 15);

        // 3 lanes * 6 = 18, within the PORT_MAX_LANES cap
        let params = UnisonParams {
            input: PolySignal::poly(&[Signal::Volts(0.0), Signal::Volts(0.0), Signal::Volts(0.0)]),
            count: 6,
            spread: None,
        };
        assert_eq!(unison_derive_lane_count(&params), 18);
    }

    #[test]
    fn test_spread_wider_than_input_cycles_the_input() {
        // Spread is the wider signal, so it sizes the output and the mono input
        // repeats across its lanes. Every declared lane must be written —
        // an unwritten one would read as a silent 0 V forever.
        let params = UnisonParams {
            input: PolySignal::mono(Signal::Volts(0.0)),
            count: 2,
            spread: Some(PolySignal::poly(&[
                Signal::Volts(10.0), // poly lane 0: 1.0 V/Oct detune
                Signal::Volts(0.0),  // poly lane 1: no detune
                Signal::Volts(10.0), // poly lane 2: 1.0 V/Oct detune
                Signal::Volts(0.0),  // poly lane 3: no detune
            ])),
        };
        assert_eq!(unison_derive_lane_count(&params), 8);

        let mut u = make_unison(params);
        u.update(48000.0);
        assert_eq!(u.outputs.sample.lanes(), 8);
        // The 0 V input is cycled into all four spread lanes.
        let expected = [-1.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0];
        for (lane, want) in expected.iter().enumerate() {
            assert!(
                (u.outputs.sample.get(lane) - want).abs() < 1e-6,
                "lane {lane}: expected {want}, got {}",
                u.outputs.sample.get(lane)
            );
        }
    }

    #[test]
    fn test_input_wider_than_spread_cycles_the_spread() {
        // The mirror case: input is wider, so the mono spread repeats across it.
        let params = UnisonParams {
            input: PolySignal::poly(&[Signal::Volts(0.0), Signal::Volts(1.0), Signal::Volts(2.0)]),
            count: 2,
            spread: Some(PolySignal::mono(Signal::Volts(10.0))),
        };
        assert_eq!(unison_derive_lane_count(&params), 6);

        let mut u = make_unison(params);
        u.update(48000.0);
        assert_eq!(u.outputs.sample.lanes(), 6);
        // Every input lane gets the same ±1.0 V/Oct detune.
        let expected = [-1.0, 0.0, 1.0, 1.0, 2.0, 3.0];
        for (lane, want) in expected.iter().enumerate() {
            assert!(
                (u.outputs.sample.get(lane) - want).abs() < 1e-6,
                "lane {lane}: expected {want}, got {}",
                u.outputs.sample.get(lane)
            );
        }
    }

    #[test]
    fn test_spread_cycles_across_input_lanes() {
        // 2-lane input, 2-lane spread with different values
        let mut u = make_unison(UnisonParams {
            input: PolySignal::poly(&[Signal::Volts(0.0), Signal::Volts(0.0)]),
            count: 2,
            spread: Some(PolySignal::poly(&[
                Signal::Volts(10.0), // lane 0: 1.0 V/Oct detune
                Signal::Volts(0.0),  // lane 1: 0.0 V/Oct detune
            ])),
        });
        u.update(48000.0);
        assert_eq!(u.outputs.sample.lanes(), 4);
        // Voice 0: input lane 0 (spread 10V) at -1.0, input lane 1 (spread 0V) at 0.0
        assert!((u.outputs.sample.get(0) - (-1.0)).abs() < 1e-6);
        assert!((u.outputs.sample.get(1) - 0.0).abs() < 1e-6);
        // Voice 1: input lane 0 at +1.0, input lane 1 at 0.0
        assert!((u.outputs.sample.get(2) - 1.0).abs() < 1e-6);
        assert!((u.outputs.sample.get(3) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_voices_interleave_input_lanes() {
        // 3-lane input, count=3 -> lane order 1 2 3 1 2 3 1 2 3
        let mut u = make_unison(UnisonParams {
            input: PolySignal::poly(&[Signal::Volts(1.0), Signal::Volts(2.0), Signal::Volts(3.0)]),
            count: 3,
            spread: None,
        });
        u.update(48000.0);
        assert_eq!(u.outputs.sample.lanes(), 9);
        let expected = [1.0, 2.0, 3.0, 1.0, 2.0, 3.0, 1.0, 2.0, 3.0];
        for (lane, want) in expected.iter().enumerate() {
            assert_eq!(u.outputs.sample.get(lane), *want, "lane {lane}");
        }
    }

    #[test]
    fn test_clamp_drops_highest_voices() {
        // 16-lane input, count=5 -> 80 desired; the cap keeps 4 full voices,
        // each a complete copy of the input lanes.
        let input: Vec<Signal> = (0..16).map(|i| Signal::Volts(i as f32)).collect();
        let mut u = make_unison(UnisonParams {
            input: PolySignal::poly(&input),
            count: 5,
            spread: None,
        });
        u.update(48000.0);
        for lane in 0..PORT_MAX_LANES {
            assert_eq!(
                u.outputs.sample.get(lane),
                (lane % 16) as f32,
                "lane {lane}"
            );
        }
    }
}
