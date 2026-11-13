use std::ops::Range;

use super::*;
use crate::dsp::drums::input::{Cached, Hold, Input, Live, each_slot};
use crate::dsp::drums::tr808::circuit::PULSE_SECS;
use crate::dsp::drums::tr808::levels;
use crate::dsp::drums::voice::{Timing, rails};
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct Rs808Params {
    #[input("rising edges strike the drum", type = trig)]
    trigger: PolySignal,
    #[input(
        "how hard the hit lands, read at each strike: up to twice as loud at 5 V",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input(
        "pitch of the upper tone, the lower one following; stock is ≈1.79 kHz; its level shifts as you tune far from stock",
        type = pitch,
        default = 2.771,
    )]
    #[deserr(default)]
    freq: Option<PolySignal>,
    #[input(
        "start every hit from rest, fading out one still sounding over 2 ms; off, each hit builds on the last, as on the 808"
    )]
    #[serde(default)]
    #[deserr(default)]
    restart: bool,
    #[input(
        "sample the controls on each strike and hold them until the next, as accent always is, so changes reach only the hits after them"
    )]
    #[serde(default)]
    #[deserr(default)]
    hold: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Rs808Outputs {
    #[output("output", "drum output; an unaccented hit peaks near ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const RS808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 rim shot",
    description: "The 808's rim shot: a bright, woody crack, over in about 10 ms, for \
                  clicky syncopation on top of a groove.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Syncopated rim clicks",
        code: r#"
            $tr808.kick($p('c*4')).out()
            $tr808.rimShot($p('c(3,8)')).out()
        "#,
    }],
};

#[module(
    name = "$tr808.rimShot",
    docs = RS808_DOCS,
    has_init,
    args(trigger),
    block
)]
pub struct Rs808 {
    outputs: Rs808Outputs,
    params: Rs808Params,
    state: RsState,
    lane_state: Box<[RimLane]>,
}

/// The rim shot's per-sample-rate constants and its held `freq`.
#[derive(Default)]
struct RsState {
    shared: RimShared,
    hold: Hold<1, 1>,
}

/// The claves' per-sample-rate constants and their held `freq` and `decay`.
#[derive(Default)]
struct ClState {
    shared: ClavesShared,
    hold: Hold<1, 2>,
}

/// A rim shot lane: its circuits, and its resonators tuned to its
/// `freq` with the output gain levelling it.
#[derive(Clone, Copy, Default)]
struct RimLane {
    voices: Voices<Rim>,
    controls: Cached<f32, (RimControls, f32)>,
}

/// A claves lane: its circuits, and its resonator tuned to its `freq`
/// and `decay` with the output gain levelling `freq`.
#[derive(Clone, Copy, Default)]
struct ClavesLane {
    voices: Voices<Claves>,
    t: Cached<(f32, f32), (BridgedT, f32)>,
}

fn rim_rest_hz() -> f32 {
    RIM_HIGH_T.rest_hz(1.0 / R312)
}

impl Rs808 {
    fn init(&mut self, sample_rate: f32) {
        let sh = &mut self.state.shared;
        sh.timing = Timing::new(PULSE_SECS, sample_rate);
        sh.substeps = (RIM_RATE / sample_rate).round().max(1.0) as u32;
        let rate = sample_rate * sh.substeps as f32;
        sh.dt = 1.0 / rate;
        sh.c24_charge = 1.0 - (-sh.dt / (R107 * C24)).exp();
        sh.d63 = d63_table();
        // Backward Euler on the high-pass's capacitors, with
        // C121·v1' = v2/R322 + (E − v1 − v2)/R325 and
        // C122·v2' = (E − v1 − v2)/R325.
        let (a, b) = (sh.dt / C121, sh.dt / C122);
        let (m11, m12, m21, m22) = (
            1.0 + a / R325,
            a / R325 - a / R322,
            b / R325,
            1.0 + b / R325,
        );
        let det = m11 * m22 - m12 * m21;
        sh.crack_inv = [m22 / det, -m12 / det, -m21 / det, m11 / det];
        let (d1, d2) = (a / R325, b / R325);
        sh.crack_dir = ((m22 * d1 - m12 * d2) / det, (m11 * d2 - m21 * d1) / det);
        sh.couple_coeff = one_pole_coeff(rc_hz(R324 + R326, C125), rate);
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        let inputs = [
            Live::new(&p.trigger),
            Live::or(&p.accent, Rs808Params::ACCENT_DEFAULT),
            Live::or(&p.freq, Rs808Params::FREQ_DEFAULT),
        ];
        let out = &mut self.outputs.sample;
        rim_tick(
            &mut self.state,
            &mut self.lane_state[..lanes],
            p.restart,
            p.hold,
            inputs,
            |lane, v| out.set(lane, v),
        );
    }

    fn process_block(
        &mut self,
        outputs: &mut Rs808BlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let Rs808ParamsBlockInputs {
            trigger,
            accent,
            freq,
        } = p.block_inputs(range.clone(), lanes, &mut scratch)?;
        let lane_state = &mut self.lane_state[..lanes];
        each_slot(
            &mut outputs.sample,
            range,
            [trigger, accent, freq],
            |inputs, mut slot| {
                rim_tick(
                    &mut self.state,
                    lane_state,
                    p.restart,
                    p.hold,
                    inputs,
                    |lane, v| slot.set(lane, v),
                )
            },
        );
        Some(())
    }
}

/// Runs one sample of every rim shot lane on its `[trigger, accent,
/// freq]`, handing `out` each one's output.
#[inline(always)]
fn rim_tick<I: Input>(
    state: &mut RsState,
    lanes: &mut [RimLane],
    restart: bool,
    hold: bool,
    [trigger, accent, freq]: [I; 3],
    mut out: impl FnMut(usize, f32),
) {
    let [freq] = state.hold.apply(hold, lanes.len(), [trigger], [freq]);
    let sh = &state.shared;
    for (lane, cs) in lanes.iter_mut().enumerate() {
        // `freq` scales both resonators and keeps their stock decays.
        let (controls, level) = cs.controls.get(freq.at(lane), |freq| {
            let r = voct_to_hz(freq) / rim_rest_hz();
            let controls = RimControls {
                high: RIM_HIGH_T.tuned(r, T60_TO_RATE / RIM_HIGH_T.decay_rate()),
                low: RIM_LOW_T.tuned(r, T60_TO_RATE / RIM_LOW_T.decay_rate()),
            };
            (controls, levels::RIM_SHOT.gain(freq))
        });
        let v = cs.voices.process(
            trigger.at(lane),
            accent,
            lane,
            restart,
            sh.timing.fade_samples,
            sh,
            &controls,
        );
        out(lane, rails(v * RIM_GAIN * level));
    }
}

message_handlers!(impl Rs808 {});

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, LaneCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct Cl808Params {
    #[input("rising edges strike the drum", type = trig)]
    trigger: PolySignal,
    #[input(
        "how hard the hit lands, read at each strike: up to twice as loud at 5 V",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input(
        "pitch; stock is ≈2.53 kHz; its level shifts as you tune far from stock",
        type = pitch,
        default = 3.273,
    )]
    #[deserr(default)]
    freq: Option<PolySignal>,
    #[input(
        "how long the claves ring, from ≈0.025 s to ≈0.23 s; ≈0.075 s at 2.5, as on the 808",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "start every hit from rest, fading out one still sounding over 2 ms; off, each hit builds on the last, as on the 808"
    )]
    #[serde(default)]
    #[deserr(default)]
    restart: bool,
    #[input(
        "sample the controls on each strike and hold them until the next, as accent always is, so changes reach only the hits after them"
    )]
    #[serde(default)]
    #[deserr(default)]
    hold: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Cl808Outputs {
    #[output("output", "drum output; an unaccented hit peaks near ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
}

const CL808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 claves",
    description: "The 808's claves: a short, pure wooden click ringing high, cutting \
                  through a mix for Latin patterns and bright accents.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Son clave",
        code: r#"
            $tr808.claves($p('c ~ ~ c ~ ~ c ~ ~ ~ c ~ c ~ ~ ~')).out()
        "#,
    }],
};

#[module(
    name = "$tr808.claves",
    docs = CL808_DOCS,
    has_init,
    args(trigger),
    block
)]
pub struct Cl808 {
    outputs: Cl808Outputs,
    params: Cl808Params,
    state: ClState,
    lane_state: Box<[ClavesLane]>,
}

impl Cl808 {
    fn init(&mut self, sample_rate: f32) {
        let sh = &mut self.state.shared;
        sh.timing = Timing::new(PULSE_SECS, sample_rate);
        sh.dt = 1.0 / sample_rate;
        sh.couple_coeff = one_pole_coeff(rc_hz(R319 + R326, C125), sample_rate);
        sh.rest_hz = CLAVES_T.rest_hz(1.0 / R312);
    }

    fn update(&mut self, _sample_rate: f32) {
        let lanes = self.lane_count();
        let p = &self.params;
        let inputs = [
            Live::new(&p.trigger),
            Live::or(&p.accent, Cl808Params::ACCENT_DEFAULT),
            Live::or(&p.freq, Cl808Params::FREQ_DEFAULT),
            Live::or(&p.decay, Cl808Params::DECAY_DEFAULT),
        ];
        let out = &mut self.outputs.sample;
        claves_tick(
            &mut self.state,
            &mut self.lane_state[..lanes],
            p.restart,
            p.hold,
            inputs,
            |lane, v| out.set(lane, v),
        );
    }

    fn process_block(
        &mut self,
        outputs: &mut Cl808BlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let Cl808ParamsBlockInputs {
            trigger,
            accent,
            freq,
            decay,
        } = p.block_inputs(range.clone(), lanes, &mut scratch)?;
        let lane_state = &mut self.lane_state[..lanes];
        each_slot(
            &mut outputs.sample,
            range,
            [trigger, accent, freq, decay],
            |inputs, mut slot| {
                claves_tick(
                    &mut self.state,
                    lane_state,
                    p.restart,
                    p.hold,
                    inputs,
                    |lane, v| slot.set(lane, v),
                )
            },
        );
        Some(())
    }
}

/// Runs one sample of every claves lane on its `[trigger, accent, freq,
/// decay]`, handing `out` each one's output.
#[inline(always)]
fn claves_tick<I: Input>(
    state: &mut ClState,
    lanes: &mut [ClavesLane],
    restart: bool,
    hold: bool,
    [trigger, accent, freq, decay]: [I; 4],
    mut out: impl FnMut(usize, f32),
) {
    let [freq, decay] = state
        .hold
        .apply(hold, lanes.len(), [trigger], [freq, decay]);
    let sh = &state.shared;
    for (lane, cs) in lanes.iter_mut().enumerate() {
        let (t, level) = cs.t.get((freq.at(lane), decay.at(lane)), |(freq, decay)| {
            let r = voct_to_hz(freq) / sh.rest_hz;
            let t = CLAVES_T.tuned(r, decay_around(decay, CLAVES_DECAY));
            (t, levels::CLAVES.gain(freq))
        });
        let v = cs.voices.process(
            trigger.at(lane),
            accent,
            lane,
            restart,
            sh.timing.fade_samples,
            sh,
            &t,
        );
        out(lane, rails(v * CLAVES_GAIN * level));
    }
}

message_handlers!(impl Cl808 {});
