//! The hi-hat's two voices as drums of their own: the closed hat, which
//! runs the lane's oscillator bank and Q23, and the open hat, which reads
//! them through its lane after the closed hat steps.

use super::*;
use crate::dsp::drums::group::{Drum, LaneInputs, QUAD};
use crate::dsp::drums::input::{Cached, Input};
use crate::dsp::drums::simd::Elementwise;
use crate::dsp::drums::tr808::metal::{MetalBank, MetalFilters};
use crate::dsp::drums::voice::{Voices, knob};

/// The decay pot, VR3: 2 MΩ linear, after R132.
const R132: f32 = 100.0e3;
const VR3: f32 = 2.0e6;

/// One lane of both hats.
#[derive(Clone, Copy, Default)]
pub(super) struct Lane {
    closed: Voices<Closed>,
    open: Voices<Open>,
    bank: MetalBank,
    cut: Cached<f32, (f32, f32)>,
    /// This sample's upper band, Q23's base current, and the resistance Q23
    /// drains the open hat through, which the closed hat hands the open.
    band: f32,
    q23: f32,
    drain: f32,
}

impl Lane {
    pub(super) fn seed(&mut self, base: u64, lane: usize) {
        self.bank.seed(base, lane);
    }
}

/// The closed hat at every width, with its oscillator bank.
pub(super) struct ClosedHat;

impl Drum for ClosedHat {
    type Shared = HatShared;
    type Circuit<L: Simd> = Closed<L>;
    type Controls<L: Simd> = ClosedControls<L>;
    type Sources<L: Simd> = MetalBank<L>;
    const RUNS_IN_QUADS: bool = true;
    const RUNS_IN_PAIRS: bool = false;
}

/// The open hat at every width.
pub(super) struct OpenHat;

impl Drum for OpenHat {
    type Shared = HatShared;
    type Circuit<L: Simd> = Open<L>;
    type Controls<L: Simd> = OpenControls<L>;
    type Sources<L: Simd> = ();
    const RUNS_IN_QUADS: bool = true;
    const RUNS_IN_PAIRS: bool = true;
}

/// The inputs every lane of the closed hat reads.
pub(super) struct ClosedSide<'a, I> {
    pub trigger: I,
    pub cut: I,
    pub filters: &'a MetalFilters,
}

impl<I: Input> ClosedSide<'_, I> {
    /// Lane `lane`'s Q23 base conductance, keeping its drain resistance
    /// for the open hat.
    #[inline(always)]
    fn cut(&self, lane: usize, cs: &mut Lane) -> f32 {
        let (q23, drain) = cs.cut.get(self.cut.at(lane), cut);
        cs.drain = drain;
        q23
    }
}

impl<I: Input> LaneInputs<ClosedHat> for ClosedSide<'_, I> {
    type Lane = Lane;

    fn voices(lane: &mut Lane) -> &mut Voices<Closed> {
        &mut lane.closed
    }

    #[inline(always)]
    fn inputs(&self, lane: usize, cs: &mut Lane) -> (f32, ClosedControls) {
        let (band, _) = cs.bank.step(self.filters);
        cs.band = band;
        let q23 = self.cut(lane, cs);
        (self.trigger.at(lane), ClosedControls { band, q23 })
    }

    #[inline]
    fn group_inputs<L: Simd>(
        &self,
        first: usize,
        members: &mut [Lane],
        bank: &mut MetalBank<L>,
        triggers: &mut [f32; QUAD],
    ) -> ClosedControls<L> {
        let (band, _) = bank.step(self.filters);
        let mut q23 = c::<L::S>(0.0);
        for (element, cs) in members.iter_mut().enumerate() {
            let lane = first + element;
            triggers[element] = self.trigger.at(lane);
            cs.band = band.element(element);
            q23.set_element(element, &self.cut(lane, cs));
        }
        ClosedControls { band, q23 }
    }

    #[inline]
    fn stepped(cs: &mut Lane) {
        cs.q23 = cs.closed.voice.q23();
    }

    #[inline]
    fn group_stepped<L: Simd>(members: &mut [Lane], voice: &Closed<L>, sounding: u32) {
        let q23 = voice.q23();
        for (element, cs) in members.iter_mut().enumerate() {
            cs.q23 = if sounding & (1 << element) != 0 {
                q23.element(element)
            } else {
                0.0
            };
        }
    }

    fn load_sources<L: Simd>(cs: &mut Lane, element: usize, bank: &mut MetalBank<L>) {
        bank.set_element(element, &cs.bank);
    }

    fn unload_sources<L: Simd>(cs: &mut Lane, element: usize, bank: &MetalBank<L>) {
        cs.bank = bank.element(element);
    }
}

/// The inputs every lane of the open hat reads.
pub(super) struct OpenSide<I> {
    pub trigger: I,
    pub decay: I,
}

impl<I: Input> LaneInputs<OpenHat> for OpenSide<I> {
    type Lane = Lane;

    fn voices(lane: &mut Lane) -> &mut Voices<Open> {
        &mut lane.open
    }

    #[inline(always)]
    fn inputs(&self, lane: usize, cs: &mut Lane) -> (f32, OpenControls) {
        let decay = knob(self.decay.at(lane));
        let controls = OpenControls {
            band: cs.band,
            decay: 1.0 / (R132 + VR3 * decay),
            q23: cs.q23,
            drain: cs.drain,
        };
        (self.trigger.at(lane), controls)
    }
}
