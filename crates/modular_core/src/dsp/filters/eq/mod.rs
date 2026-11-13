//! `$eq`: serial parametric EQ of trapezoidal state-variable filter
//! sections.

use std::ops::Range;

use schemars::JsonSchema;

use crate::block_input::{BlockInput, InputScratch};
use crate::dsp::filters::svf_core;
use crate::dsp::utils::sanitize;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, DocSection, ModuleDocs};
use crate::poly::{LaneSum, PolyOutput};

mod live;
mod params;
mod svf;

pub use live::eq_state_builder;

use params::{DEFAULT_Q, EqSections, MAX_EQ_SECTIONS, ParametricEqParams, eq_derive_lane_count};
use svf::SectionRuntime;

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ParametricEqOutputs {
    #[output("output", "equalized signal", type = cv, default)]
    sample: PolyOutput,
}

#[derive(Clone, Copy)]
struct ParametricEqLane {
    /// Per-section integrator states.
    ic1: [f32; MAX_EQ_SECTIONS],
    ic2: [f32; MAX_EQ_SECTIONS],
    /// In the per-lane form, the coefficients of list `c` live on lane
    /// `c`, and every lane reads its list's from there.
    per: [SectionRuntime; MAX_EQ_SECTIONS],
}

impl Default for ParametricEqLane {
    fn default() -> Self {
        Self {
            ic1: [0.0; MAX_EQ_SECTIONS],
            ic2: [0.0; MAX_EQ_SECTIONS],
            per: [SectionRuntime::default(); MAX_EQ_SECTIONS],
        }
    }
}

struct ParametricEqState {
    /// Coefficients of the shared section list, used by every lane.
    mono: [SectionRuntime; MAX_EQ_SECTIONS],
    /// The runtimes that ran last; the others hold glide state from an older
    /// form. None before the first patch update.
    active: Option<ActiveRuntimes>,
}

/// Which runtimes a sections form drives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveRuntimes {
    /// `state.mono`.
    Shared,
    /// `per` on the first this-many lanes.
    PerLane(usize),
}

impl Default for ParametricEqState {
    fn default() -> Self {
        Self {
            mono: [SectionRuntime::default(); MAX_EQ_SECTIONS],
            active: None,
        }
    }
}

const EQ_DOCS: ModuleDocs = ModuleDocs {
    summary: "Parametric EQ",
    description: "Shapes tone with up to 8 bands of peaks, shelves, cuts and notches, any of \
                  which can be modulated. Give `sections` one list of bands per lane to EQ \
                  left and right differently, or set `ui: true` to shape it by hand.",
    categories: &[ModuleCategory::Filter],
    sections: &[DocSection {
        title: "Editing in the EQ editor",
        body: "Drag a band's dot to move it, scroll over it or drag its handles to change `q`, \
               and double-click to add a band. Changes play right away and are written into \
               your code. Hold Ctrl/Cmd to change only the code, and right-click a control to \
               reset it.",
        table: None,
        examples: &[DocExample {
            title: "Start empty and build the EQ by hand",
            code: "$saw('c2').eq({ ui: true }).out()",
        }],
    }],
    examples: &[
        DocExample {
            title: "Warm lows with a sweeping mid dip",
            code: r#"
                $saw('c2')
                  .eq({
                    sections: [
                      { type: 'lowShelf', freq: '150hz', gain: 3 },
                      {
                        type: 'peak',
                        freq: $sine('0.5hz').range('400hz', '2000hz'),
                        gain: -6,
                        q: 2,
                      },
                      { type: 'highpass', freq: '40hz' },
                    ],
                    adaptiveQ: true,
                  })
                  .out()
            "#,
        },
        DocExample {
            title: "A darker left lane, a brighter right one",
            code: r#"
                $saw(['c2', 'g2'])
                  .eq({
                    sections: [
                      [{ type: 'highShelf', freq: '3000hz', gain: -6 }],
                      [{ type: 'highShelf', freq: '3000hz', gain: 4 }],
                    ],
                  })
                  .out()
            "#,
        },
    ],
};

#[module(name = "$eq", docs = EQ_DOCS, lanes_derive = eq_derive_lane_count, patch_update, stateful, args(input), block)]
pub struct ParametricEq {
    outputs: ParametricEqOutputs,
    state: ParametricEqState,
    lane_state: Box<[ParametricEqLane]>,
    params: ParametricEqParams,
}

impl ParametricEq {
    fn update(&mut self, sample_rate: f32) {
        let lanes = self.lane_count();
        let adaptive_q = self.params.adaptive_q;
        match &self.params.sections {
            EqSections::Shared(list) => {
                let n = list.len().min(MAX_EQ_SECTIONS);
                for (rt, sec) in self.state.mono.iter_mut().zip(&list[..n]) {
                    rt.refresh(sec, sample_rate, adaptive_q);
                }
                for lane in 0..lanes {
                    let cs = &mut self.lane_state[lane];
                    let mut x = sanitize(self.params.input.get_value(lane));
                    for s in 0..n {
                        x = svf_core::process(
                            &self.state.mono[s].coeffs,
                            &mut cs.ic1[s],
                            &mut cs.ic2[s],
                            x,
                        );
                    }
                    self.outputs.sample.set(lane, x);
                }
            }
            EqSections::PerLane(lists) => {
                // List `l` is computed once, on lane `l`'s runtimes; there
                // are never fewer lanes than lists. A stereo lane's two sides
                // share its list.
                let channels = self.channels();
                let num_lists = lists.len().min(lanes / channels);
                for (l, list) in lists.iter().take(num_lists).enumerate() {
                    let n = list.len().min(MAX_EQ_SECTIONS);
                    let per = &mut self.lane_state[l].per;
                    for (rt, sec) in per.iter_mut().zip(&list[..n]) {
                        rt.refresh(sec, sample_rate, adaptive_q);
                    }
                }
                for lane in 0..lanes {
                    let l = (lane / channels) % num_lists;
                    let n = lists[l].len().min(MAX_EQ_SECTIONS);
                    let mut x = sanitize(self.params.input.get_value(lane));
                    for s in 0..n {
                        let c = self.lane_state[l].per[s].coeffs;
                        let cs = &mut self.lane_state[lane];
                        x = svf_core::process(&c, &mut cs.ic1[s], &mut cs.ic2[s], x);
                    }
                    self.outputs.sample.set(lane, x);
                }
            }
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut ParametricEqBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let lanes = self.lane_count();
        let channels = self.channels();
        let adaptive_q = self.params.adaptive_q;
        let shared = matches!(self.params.sections, EqSections::Shared(_));
        let (lists, num_lists) = match &self.params.sections {
            EqSections::Shared(list) => (std::slice::from_ref(list), 1),
            EqSections::PerLane(lists) => (&lists[..], lists.len().min(lanes / channels)),
        };
        let mut s_in = InputScratch::new();
        let input = self
            .params
            .input
            .block_input(range.clone(), lanes, &mut s_in)?;

        // The params of every section not yet settled, in `update`'s refresh
        // order; more such sections than fit decline the range.
        let mut pool: [InputScratch; 3 * MAX_EQ_SECTIONS] =
            std::array::from_fn(|_| InputScratch::new());
        let mut free = pool.iter_mut();
        let mut moving = [None; MAX_EQ_SECTIONS];
        let mut count = 0;
        for (l, list) in lists.iter().take(num_lists).enumerate() {
            let n = list.len().min(MAX_EQ_SECTIONS);
            for (s, sec) in list[..n].iter().enumerate() {
                let rt = if shared {
                    &self.state.mono[s]
                } else {
                    &self.lane_state[l].per[s]
                };
                if rt.is_settled() {
                    continue;
                }
                let targets = [
                    Target::resolve(Some(&sec.freq), 0.0, &range, free.next()?)?,
                    Target::resolve(sec.gain.as_ref(), 0.0, &range, free.next()?)?,
                    Target::resolve(sec.q.as_ref(), DEFAULT_Q, &range, free.next()?)?,
                ];
                *moving.get_mut(count)? = Some((l, s, targets));
                count += 1;
            }
        }

        for i in range {
            for &(l, s, [f, g, q]) in moving[..count].iter().flatten() {
                let rt = if shared {
                    &mut self.state.mono[s]
                } else {
                    &mut self.lane_state[l].per[s]
                };
                if !rt.is_settled() {
                    rt.advance(
                        &lists[l][s],
                        [f.at(i), g.at(i), q.at(i)],
                        sample_rate,
                        adaptive_q,
                    );
                }
            }
            let row = input.row(i);
            let slot = outputs.sample.slot_mut(i);
            for (lane, (out, &x)) in slot.iter_mut().zip(row).enumerate() {
                let l = (lane / channels) % num_lists;
                let n = lists[l].len().min(MAX_EQ_SECTIONS);
                let mut x = sanitize(x);
                for s in 0..n {
                    let c = if shared {
                        self.state.mono[s].coeffs
                    } else {
                        self.lane_state[l].per[s].coeffs
                    };
                    let cs = &mut self.lane_state[lane];
                    x = svf_core::process(&c, &mut cs.ic1[s], &mut cs.ic2[s], x);
                }
                *out = sanitize(x);
            }
        }
        Some(())
    }
}

/// A section param resolved for a block range: the lanes it sums, or its
/// default when absent.
#[derive(Clone, Copy)]
enum Target<'a> {
    Summed(BlockInput<'a>),
    Default(f32),
}

impl<'a> Target<'a> {
    fn resolve(
        signal: Option<&'a LaneSum>,
        default: f32,
        range: &Range<usize>,
        scratch: &'a mut InputScratch,
    ) -> Option<Self> {
        let Some(signal) = signal else {
            return Some(Target::Default(default));
        };
        let poly = signal.poly();
        poly.block_input(range.clone(), poly.lanes(), scratch)
            .map(Target::Summed)
    }

    /// Slot `i`'s value, summed as [`LaneSum::get_value`] sums.
    #[inline(always)]
    fn at(&self, i: usize) -> f32 {
        match self {
            Target::Summed(rows) => rows.row(i).iter().sum(),
            Target::Default(value) => *value,
        }
    }
}

impl crate::types::PatchUpdateHandler for ParametricEq {
    /// Section types, section order, the sections form, and adaptive Q
    /// change only on patch updates and are invisible to the per-sample
    /// change gate, so every section recomputes its coefficients on the next
    /// sample.
    ///
    /// Runtimes coming back into use after another form ran hold stale
    /// glide state, so they start fresh and snap to the new values.
    fn on_patch_update(&mut self) {
        let now = match &self.params.sections {
            EqSections::Shared(_) => ActiveRuntimes::Shared,
            EqSections::PerLane(lists) => {
                ActiveRuntimes::PerLane(lists.len().min(self.lane_count() / self.channels()))
            }
        };
        let fresh_lists = match (self.state.active, now) {
            (Some(ActiveRuntimes::Shared), ActiveRuntimes::Shared) => 0..0,
            (_, ActiveRuntimes::Shared) => {
                self.state.mono = [SectionRuntime::default(); MAX_EQ_SECTIONS];
                0..0
            }
            (Some(ActiveRuntimes::PerLane(was)), ActiveRuntimes::PerLane(n)) => was.min(n)..n,
            (_, ActiveRuntimes::PerLane(n)) => 0..n,
        };
        for cs in &mut self.lane_state[fresh_lists] {
            cs.per = [SectionRuntime::default(); MAX_EQ_SECTIONS];
        }
        self.state.active = Some(now);

        for rt in self.state.mono.iter_mut() {
            rt.invalidate();
        }
        for cs in self.lane_state.iter_mut() {
            for rt in cs.per.iter_mut() {
                rt.invalidate();
            }
        }
    }
}

impl crate::types::StatefulModule for ParametricEq {
    /// Publish each list's running section values for the editor panel.
    fn write_module_state(&self, out: &mut dyn crate::module_state::ModuleLiveState) {
        let Some(out) = out.as_any_mut().downcast_mut::<live::EqLiveState>() else {
            return;
        };
        crate::module_state::ModuleLiveState::reset(out);
        match &self.params.sections {
            EqSections::Shared(list) => {
                let n = list.len().min(MAX_EQ_SECTIONS);
                out.write_list(0, self.state.mono[..n].iter().map(|rt| rt.live_values()));
            }
            EqSections::PerLane(lists) => {
                let num_lists = lists.len().min(self.lane_count() / self.channels());
                for (l, list) in lists.iter().take(num_lists).enumerate() {
                    let n = list.len().min(MAX_EQ_SECTIONS);
                    let per = &self.lane_state[l].per[..n];
                    out.write_list(l, per.iter().map(|rt| rt.live_values()));
                }
            }
        }
    }
}

message_handlers!(impl ParametricEq {});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly::PolySignal;
    use crate::types::{OutputStruct, Signal};
    use params::{DEFAULT_Q, EqSection, EqSectionType};

    fn make(sections: Vec<EqSection>) -> ParametricEq {
        make_with(EqSections::Shared(sections), 1)
    }

    fn make_with(sections: EqSections, lanes: usize) -> ParametricEq {
        let mut outputs = ParametricEqOutputs::default();
        outputs.set_all_lanes(lanes);
        ParametricEq {
            params: ParametricEqParams {
                input: PolySignal::mono(Signal::Volts(0.0)),
                sections,
                adaptive_q: false,
                ui: false,
            },
            outputs,
            _lane_count: lanes,
            _channels: 1,
            _block_index: Default::default(),
            state: ParametricEqState::default(),
            lane_state: vec![ParametricEqLane::default(); lanes].into_boxed_slice(),
        }
    }

    fn section(kind: EqSectionType, gain_db: f32, q: f32) -> EqSection {
        EqSection {
            r#type: kind,
            freq: PolySignal::mono(Signal::Volts(0.0)).into(),
            gain: Some(PolySignal::mono(Signal::Volts(gain_db)).into()),
            q: Some(PolySignal::mono(Signal::Volts(q)).into()),
        }
    }

    /// RMS of the module output for a unit sine at `freq_hz`, skipping the
    /// first 1000 samples of filter transient.
    fn sine_rms(m: &mut ParametricEq, freq_hz: f32, n: usize) -> f32 {
        let mut acc = 0.0f64;
        let mut count = 0usize;
        for i in 0..n {
            let x = (std::f32::consts::TAU * freq_hz * i as f32 / 48000.0).sin();
            m.params.input = PolySignal::mono(Signal::Volts(x));
            m.update(48000.0);
            if i >= 1000 {
                let y = m.outputs.sample.get(0) as f64;
                acc += y * y;
                count += 1;
            }
        }
        (acc / count as f64).sqrt() as f32
    }

    const C4_HZ: f32 = 261.63;
    const UNIT_SINE_RMS: f32 = std::f32::consts::FRAC_1_SQRT_2;

    #[test]
    fn peak_boost_raises_center_leaves_distant_band() {
        let mut m = make(vec![section(EqSectionType::Peak, 12.0, 1.0)]);
        let center = sine_rms(&mut m, C4_HZ, 20_000);
        assert!(
            center > 2.5 * UNIT_SINE_RMS,
            "+12 dB peak should raise center RMS, got {center}"
        );
        let mut m = make(vec![section(EqSectionType::Peak, 12.0, 1.0)]);
        let distant = sine_rms(&mut m, 8000.0, 20_000);
        assert!(
            (distant - UNIT_SINE_RMS).abs() < 0.2 * UNIT_SINE_RMS,
            "distant band should be near unity, got {distant}"
        );
    }

    #[test]
    fn notch_kills_center() {
        let mut m = make(vec![section(EqSectionType::Notch, 0.0, 4.0)]);
        let center = sine_rms(&mut m, C4_HZ, 20_000);
        assert!(
            center < 0.25 * UNIT_SINE_RMS,
            "notch should attenuate center, got {center}"
        );
        let mut m = make(vec![section(EqSectionType::Notch, 0.0, 4.0)]);
        let distant = sine_rms(&mut m, 8000.0, 20_000);
        assert!(
            (distant - UNIT_SINE_RMS).abs() < 0.2 * UNIT_SINE_RMS,
            "distant band should be near unity, got {distant}"
        );
    }

    #[test]
    fn zero_gain_peak_is_identity() {
        let mut m = make(vec![section(EqSectionType::Peak, 0.0, 1.0)]);
        let rms = sine_rms(&mut m, C4_HZ, 20_000);
        assert!(
            (rms - UNIT_SINE_RMS).abs() < 0.02 * UNIT_SINE_RMS,
            "0 dB peak should pass through, got {rms}"
        );
    }

    #[test]
    fn adaptive_q_narrows_boosted_peak() {
        let rms_pair = |adaptive: bool| {
            let mut m = make(vec![section(EqSectionType::Peak, 12.0, 1.0)]);
            m.params.adaptive_q = adaptive;
            let center = sine_rms(&mut m, C4_HZ, 20_000);
            let mut m = make(vec![section(EqSectionType::Peak, 12.0, 1.0)]);
            m.params.adaptive_q = adaptive;
            let octave_up = sine_rms(&mut m, 2.0 * C4_HZ, 20_000);
            (center, octave_up)
        };
        let (fixed_center, fixed_skirt) = rms_pair(false);
        let (adaptive_center, adaptive_skirt) = rms_pair(true);
        assert!(
            (adaptive_center - fixed_center).abs() < 0.02 * fixed_center,
            "adaptive Q keeps the peak gain, got {adaptive_center} vs {fixed_center}"
        );
        assert!(
            adaptive_skirt < 0.8 * fixed_skirt,
            "adaptive Q narrows the bell, skirt {adaptive_skirt} vs {fixed_skirt}"
        );
    }

    #[test]
    fn adaptive_q_leaves_shelves_unchanged() {
        let rms_at_f0 = |adaptive: bool| {
            let mut m = make(vec![section(EqSectionType::LowShelf, 12.0, DEFAULT_Q)]);
            m.params.adaptive_q = adaptive;
            sine_rms(&mut m, C4_HZ, 20_000)
        };
        assert!((rms_at_f0(true) - rms_at_f0(false)).abs() < 1e-4);
    }

    /// Peak and trough of a +12 dB low shelf's response across ±2 octaves.
    fn shelf_extremes(q: f32) -> (f32, f32) {
        let (mut hi, mut lo) = (0.0f32, f32::MAX);
        for k in -6..=6 {
            let freq = C4_HZ * 2f32.powf(k as f32 / 3.0);
            let mut m = make(vec![section(EqSectionType::LowShelf, 12.0, q)]);
            let rms = sine_rms(&mut m, freq, 20_000);
            hi = hi.max(rms);
            lo = lo.min(rms);
        }
        (hi, lo)
    }

    #[test]
    fn shelf_overshoots_only_above_0_707() {
        let plateau = UNIT_SINE_RMS * 10f32.powf(12.0 / 20.0);
        let (hi, lo) = shelf_extremes(DEFAULT_Q);
        assert!(
            hi <= plateau * 1.01 && lo >= UNIT_SINE_RMS * 0.99,
            "a 0.707 shelf stays within [0, 12] dB: {lo}..{hi}"
        );
        let (hi, lo) = shelf_extremes(4.0);
        assert!(
            hi > plateau * 1.1,
            "a q 4 shelf bumps above its plateau: {hi}"
        );
        assert!(lo < UNIT_SINE_RMS * 0.9, "and dips below unity: {lo}");
    }

    #[test]
    fn patch_update_recomputes_when_only_type_changes() {
        use crate::types::PatchUpdateHandler;
        let mut m = make(vec![section(EqSectionType::Peak, 0.0, 4.0)]);
        sine_rms(&mut m, C4_HZ, 2000);
        let EqSections::Shared(list) = &mut m.params.sections else {
            unreachable!()
        };
        list[0].r#type = EqSectionType::Notch;
        m.on_patch_update();
        let center = sine_rms(&mut m, C4_HZ, 20_000);
        assert!(
            center < 0.25 * UNIT_SINE_RMS,
            "notch must take effect after the patch update, got {center}"
        );
    }

    #[test]
    fn per_lane_lists_eq_each_lane_independently() {
        // Four lanes over two lists: lanes 2 and 3 reuse lists 0 and 1.
        let sections = EqSections::PerLane(vec![
            vec![section(EqSectionType::Notch, 0.0, 4.0)],
            vec![section(EqSectionType::Peak, 0.0, 1.0)],
        ]);
        let mut m = make_with(sections, 4);
        let mut acc = [0.0f64; 4];
        for i in 0..20_000 {
            let x = (std::f32::consts::TAU * C4_HZ * i as f32 / 48000.0).sin();
            m.params.input = PolySignal::mono(Signal::Volts(x));
            m.update(48000.0);
            if i >= 1000 {
                for (lane, a) in acc.iter_mut().enumerate() {
                    *a += (m.outputs.sample.get(lane) as f64).powi(2);
                }
            }
        }
        let rms = acc.map(|a| (a / 19_000.0).sqrt() as f32);
        for lane in [0, 2] {
            assert!(
                rms[lane] < 0.25 * UNIT_SINE_RMS,
                "lane {lane} notched: {}",
                rms[lane]
            );
        }
        for lane in [1, 3] {
            assert!(
                (rms[lane] - UNIT_SINE_RMS).abs() < 0.02 * UNIT_SINE_RMS,
                "lane {lane} passes: {}",
                rms[lane]
            );
        }
    }

    #[test]
    fn publishes_running_values_per_list() {
        use crate::types::StatefulModule;
        let mut m = make_with(
            EqSections::PerLane(vec![
                vec![section(EqSectionType::Peak, 6.0, 2.0)],
                vec![
                    section(EqSectionType::Peak, -30.0, 50.0),
                    section(EqSectionType::Notch, 0.0, 1.0),
                ],
            ]),
            2,
        );
        for _ in 0..20_000 {
            m.update(48000.0);
        }
        let params = serde_json::json!({ "ui": true, "sections": [[{}], [{}, {}]] });
        let (mut live, meta) = live::eq_state_builder(&params).unwrap();
        m.write_module_state(&mut *live);
        let json = meta.build_json(&*live);
        let lists = json["eqLive"].as_array().unwrap();
        let close = |v: &serde_json::Value, want: f64| (v.as_f64().unwrap() - want).abs() < 1e-3;
        let first = &lists[0][0];
        assert!(
            close(&first[0], 0.0) && close(&first[1], 6.0) && close(&first[2], 2.0),
            "{first}"
        );
        // Gain and q are reported after clamping.
        let second = &lists[1][0];
        assert!(
            close(&second[1], -24.0) && close(&second[2], 20.0),
            "{second}"
        );
        assert_eq!(lists[1].as_array().unwrap().len(), 2);
    }

    #[test]
    fn settled_section_takes_a_changed_value_after_patch_update() {
        use crate::types::PatchUpdateHandler;
        let mut m = make(vec![section(EqSectionType::Peak, 0.0, 1.0)]);
        // Long enough for the filter to settle.
        sine_rms(&mut m, C4_HZ, 20_000);
        let EqSections::Shared(list) = &mut m.params.sections else {
            unreachable!()
        };
        list[0].gain = Some(PolySignal::mono(Signal::Volts(12.0)).into());
        m.on_patch_update();
        let center = sine_rms(&mut m, C4_HZ, 20_000);
        assert!(
            center > 2.5 * UNIT_SINE_RMS,
            "the new +12 dB gain must apply, got {center}"
        );
    }

    #[test]
    fn non_finite_input_never_corrupts_the_chain() {
        let mut m = make(vec![
            section(EqSectionType::Peak, 12.0, 4.0),
            section(EqSectionType::LowShelf, -6.0, 0.7),
        ]);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            m.params.input = PolySignal::mono(Signal::Volts(bad));
            m.update(48000.0);
            assert!(m.outputs.sample.get(0).is_finite());
        }
        let rms = sine_rms(&mut m, 8000.0, 20_000);
        assert!(
            rms.is_finite() && rms > 0.1,
            "the chain recovers, got {rms}"
        );
    }

    #[test]
    fn returning_to_a_form_does_not_glide_from_its_old_values() {
        use crate::types::PatchUpdateHandler;
        let band = |volts: f32| {
            let mut s = section(EqSectionType::Peak, 6.0, 1.0);
            s.freq = PolySignal::mono(Signal::Volts(volts)).into();
            s
        };
        let mut m = make_with(EqSections::Shared(vec![band(-1.0)]), 2);
        m.on_patch_update();
        sine_rms(&mut m, C4_HZ, 2000);
        m.params.sections = EqSections::PerLane(vec![vec![band(-1.0)], vec![band(-1.0)]]);
        m.on_patch_update();
        sine_rms(&mut m, C4_HZ, 2000);
        m.params.sections = EqSections::Shared(vec![band(4.0)]);
        m.on_patch_update();
        m.update(48000.0);
        let freq = m.state.mono[0].live_values()[0];
        assert!(
            (freq - 4.0).abs() < 1e-3,
            "the shared runtime starts at the new freq, got {freq} V"
        );
    }

    #[test]
    fn eight_sections_stay_finite() {
        let sections = vec![
            section(EqSectionType::LowShelf, 6.0, 0.7),
            section(EqSectionType::Peak, -12.0, 2.0),
            section(EqSectionType::Peak, 12.0, 8.0),
            section(EqSectionType::HighShelf, -6.0, 0.7),
            section(EqSectionType::Lowpass, 0.0, 2.0),
            section(EqSectionType::Highpass, 0.0, 2.0),
            section(EqSectionType::Notch, 0.0, 4.0),
            section(EqSectionType::Peak, 24.0, 20.0),
        ];
        let mut m = make(sections);
        for i in 0..8000 {
            let x = (std::f32::consts::TAU * 440.0 * i as f32 / 48000.0).sin();
            m.params.input = PolySignal::mono(Signal::Volts(x));
            m.update(48000.0);
            let y = m.outputs.sample.get(0);
            assert!(y.is_finite(), "8-section chain should stay finite, got {y}");
        }
    }
}
