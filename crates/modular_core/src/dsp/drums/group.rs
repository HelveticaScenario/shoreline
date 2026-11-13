//! Running a drum's channels in groups. Channels form blocks of four from
//! the first. A block of three or four channels runs as one [`X4`]
//! circuit, or as [`X2`] pairs, and a block of two as one pair, each where
//! the drum runs faster that way; every other channel runs alone at
//! [`Mono`] width.
//!
//! Each channel holds the [`Mono`] form of its own state. While its group
//! runs, the group holds the live form, and a patch update that changes
//! the channel count hands every group back to its channels and gathers
//! the new groups from them.

use super::input::Input;
use super::simd::{Elementwise, Mask, Mono, Real, Simd, X2, X4, c};
use super::voice::{Circuit, Fire, Level, Voices, under};
use crate::poly::PORT_MAX_CHANNELS;

/// Channels in a block, and in a full group.
pub const QUAD: usize = 4;

/// How a run of channels is grouped.
#[derive(Clone, Copy)]
enum Width {
    One,
    Pair,
    Quad,
}

/// Hands `each` the groups `channels` channels of drum `D` run in, as each
/// one's first channel, number of channels, and width.
#[inline(always)]
fn groups<D: Drum>(channels: usize, mut each: impl FnMut(usize, usize, Width)) {
    #[cfg(test)]
    if ALONE.get() {
        (0..channels).for_each(|ch| each(ch, 1, Width::One));
        return;
    }
    for first in (0..channels).step_by(QUAD) {
        let elements = (channels - first).min(QUAD);
        if elements > 2 && D::RUNS_IN_QUADS {
            each(first, elements, Width::Quad);
        } else if elements > 1 && D::RUNS_IN_PAIRS {
            each(first, 2, Width::Pair);
            match elements {
                4 => each(first + 2, 2, Width::Pair),
                3 => each(first + 2, 1, Width::One),
                _ => {}
            }
        } else {
            for ch in first..first + elements {
                each(ch, 1, Width::One);
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    /// While set, every channel on this thread runs alone, so a test can
    /// compare each channel against its own grouped run.
    pub static ALONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A drum voice's circuit at every width.
pub trait Drum: 'static {
    /// Constants shared by every channel, fixed by the sample rate.
    type Shared;
    type Circuit<L: Simd>: Circuit<L, Shared = Self::Shared, Controls = Self::Controls<L>>
        + Elementwise<Element = Self::Circuit<Mono>>;
    /// Settings read once per sample.
    type Controls<L: Simd>: Default + Elementwise<Element = Self::Controls<Mono>>;
    /// State a group runs every sample for its channels besides their
    /// circuits, such as the sources the settings come from. Each channel
    /// holds its own, in the [`Mono`] form, while it runs alone.
    type Sources<L: Simd>: Default;
    /// Whether three or four channels run faster as one [`X4`] group than
    /// one at a time.
    const RUNS_IN_QUADS: bool;
    /// Whether two channels run faster as one [`X2`] pair than one at a time.
    const RUNS_IN_PAIRS: bool;
}

/// A drum module's channels, as the module reads them each sample.
pub trait ChannelInputs<D: Drum> {
    /// One channel's state.
    type Channel;

    /// The channel's strike detector and circuits.
    fn voices(channel: &mut Self::Channel) -> &mut Voices<D::Circuit<Mono>>;
    /// Channel `ch`'s trigger and settings this sample, running alone.
    fn inputs(&self, ch: usize, channel: &mut Self::Channel) -> (f32, D::Controls<Mono>);

    /// The triggers and settings of a group of channels, `members`, from
    /// `first` on, with the group's `sources`; by default each channel's
    /// [`Self::inputs`], element by element.
    #[inline]
    fn group_inputs<L: Simd>(
        &self,
        first: usize,
        members: &mut [Self::Channel],
        _sources: &mut D::Sources<L>,
        triggers: &mut [f32; QUAD],
    ) -> D::Controls<L> {
        let mut controls = D::Controls::<L>::default();
        for (element, channel) in members.iter_mut().enumerate() {
            let (trigger, element_controls) = self.inputs(first + element, channel);
            triggers[element] = trigger;
            controls.set_element(element, &element_controls);
        }
        controls
    }

    /// Reads a channel running alone after each of its samples.
    #[inline]
    fn stepped(_channel: &mut Self::Channel) {}

    /// Reads a group's sounding circuits, `voice`, after they step, with
    /// `sounding` marking the elements of `members` they sound for.
    #[inline]
    fn group_stepped<L: Simd>(
        _members: &mut [Self::Channel],
        _voice: &D::Circuit<L>,
        _sounding: u32,
    ) {
    }

    /// Moves `channel`'s sources into element `element` of a group's.
    #[inline]
    fn load_sources<L: Simd>(
        _channel: &mut Self::Channel,
        _element: usize,
        _sources: &mut D::Sources<L>,
    ) {
    }

    /// Moves element `element` of a group's sources back into `channel`.
    #[inline]
    fn unload_sources<L: Simd>(
        _channel: &mut Self::Channel,
        _element: usize,
        _sources: &D::Sources<L>,
    ) {
    }
}

/// The settings every channel of a drum runs with this sample.
pub struct Run<'a, D: Drum, I: Input> {
    pub accent: I,
    pub restart: bool,
    pub fade_samples: u32,
    pub shared: &'a D::Shared,
}

/// A drum's groups, by their first channel: a group of four from each
/// block's first, and a pair from each block's first and third.
pub struct Groups<D: Drum> {
    /// The channel count the groups are laid out for.
    channels: usize,
    quads: Box<[VoiceGroup<X4, D>]>,
    pairs: Box<[VoiceGroup<X2, D>]>,
}

impl<D: Drum> Default for Groups<D> {
    fn default() -> Self {
        Self {
            channels: 0,
            quads: Box::default(),
            pairs: Box::default(),
        }
    }
}

impl<D: Drum> Groups<D>
where
    D::Circuit<Mono>: Fire,
{
    /// Groups for every channel count a module can have. Allocates, so it
    /// belongs in `init`.
    pub fn new() -> Self {
        let quads = if D::RUNS_IN_QUADS {
            PORT_MAX_CHANNELS / QUAD
        } else {
            0
        };
        let pairs = if D::RUNS_IN_PAIRS {
            PORT_MAX_CHANNELS / 2
        } else {
            0
        };
        Self {
            channels: 0,
            quads: (0..quads).map(|_| VoiceGroup::default()).collect(),
            pairs: (0..pairs).map(|_| VoiceGroup::default()).collect(),
        }
    }

    /// Lays out `count` channels, gathering each channel's state from where
    /// the last layout left it.
    pub fn layout<A: ChannelInputs<D>>(&mut self, channels: &mut [A::Channel], count: usize) {
        groups::<D>(self.channels, |first, elements, width| {
            let kept = &mut channels[first.min(count)..(first + elements).min(count)];
            match width {
                Width::One => {}
                Width::Pair => self.pairs[first / 2].unload::<A>(kept),
                Width::Quad => self.quads[first / QUAD].unload::<A>(kept),
            }
        });
        groups::<D>(count, |first, elements, width| {
            let members = &mut channels[first..first + elements];
            match width {
                Width::One => {}
                Width::Pair => self.pairs[first / 2].load::<A>(members),
                Width::Quad => self.quads[first / QUAD].load::<A>(members),
            }
        });
        self.channels = count;
    }

    /// Runs one sample of every channel, handing `out` each channel's
    /// output.
    #[inline]
    pub fn run<A: ChannelInputs<D>, I: Input>(
        &mut self,
        adapter: &A,
        channels: &mut [A::Channel],
        run: &Run<D, I>,
        mut out: impl FnMut(usize, f32),
    ) {
        groups::<D>(channels.len(), |first, elements, width| {
            let members = &mut channels[first..first + elements];
            match width {
                Width::One => {
                    let channel = &mut members[0];
                    let (trigger, controls) = adapter.inputs(first, channel);
                    let v = A::voices(channel).process(
                        trigger,
                        run.accent,
                        first,
                        run.restart,
                        run.fade_samples,
                        run.shared,
                        &controls,
                    );
                    A::stepped(channel);
                    out(first, v);
                }
                Width::Pair => self.pairs[first / 2].run(adapter, members, first, run, &mut out),
                Width::Quad => self.quads[first / QUAD].run(adapter, members, first, run, &mut out),
            }
        });
    }
}

/// A group's circuits at width `L`: the sounding circuits and the cut-off
/// ones fading out under them. A element whose circuit settles is parked: its
/// channel's [`Voices`] holds the circuit as it settled, as a channel
/// running alone would, while the element runs on unheard, and the channel's
/// next hit starts from the parked circuit.
struct VoiceGroup<L: Simd, D: Drum> {
    voice: D::Circuit<L>,
    tail: D::Circuit<L>,
    trim: Level<L>,
    tail_trim: L::S,
    parked: u32,
    sources: D::Sources<L>,
}

impl<L: Simd, D: Drum> Default for VoiceGroup<L, D> {
    fn default() -> Self {
        Self {
            voice: Default::default(),
            tail: Default::default(),
            trim: Default::default(),
            tail_trim: Default::default(),
            parked: u32::MAX,
            sources: Default::default(),
        }
    }
}

impl<L: Simd, D: Drum> VoiceGroup<L, D>
where
    D::Circuit<Mono>: Fire,
{
    /// Takes up the circuits of `members`, one channel per element. Elements past
    /// them rest.
    fn load<A: ChannelInputs<D>>(&mut self, members: &mut [A::Channel]) {
        *self = Self::default();
        for (element, channel) in members.iter_mut().enumerate() {
            A::load_sources(channel, element, &mut self.sources);
            let v = A::voices(channel);
            self.voice.set_element(element, &v.voice);
            self.tail.set_element(element, &v.tail);
            self.trim.set_element(element, &v.trim);
            self.tail_trim.set_element(element, &v.tail_trim);
            if !v.voice.settled() {
                self.parked &= !(1 << element);
            }
        }
    }

    /// Hands each element's circuits back to its channel in `members`.
    fn unload<A: ChannelInputs<D>>(&self, members: &mut [A::Channel]) {
        for (element, channel) in members.iter_mut().enumerate() {
            A::unload_sources(channel, element, &self.sources);
            let v = A::voices(channel);
            if self.parked & (1 << element) == 0 {
                v.voice = self.voice.element(element);
            }
            v.tail = self.tail.element(element);
            v.trim = self.trim.element(element);
            v.tail_trim = self.tail_trim.element(element);
        }
    }

    /// Runs one sample of each channel in `members`, from `first` on, as
    /// [`Voices::process`] does for one channel, handing `out` each
    /// channel's output.
    #[inline(never)]
    fn run<A: ChannelInputs<D>, I: Input>(
        &mut self,
        adapter: &A,
        members: &mut [A::Channel],
        first: usize,
        run: &Run<D, I>,
        out: &mut impl FnMut(usize, f32),
    ) {
        let live = (1u32 << members.len()) - 1;
        let trim = <D::Circuit<Mono> as Fire>::accent_trim(run.shared) - 1.0;
        let mut triggers = [0.0; QUAD];
        let mut controls = adapter.group_inputs(first, members, &mut self.sources, &mut triggers);
        for (element, channel) in members.iter_mut().enumerate() {
            let v = A::voices(channel);
            let Some(accent) = v
                .strike
                .process(triggers[element], run.accent, first + element)
            else {
                continue;
            };
            let mut circuit = if self.parked & (1 << element) != 0 {
                v.voice
            } else {
                self.voice.element(element)
            };
            if run.restart && !circuit.settled() {
                self.tail.set_element(element, &circuit);
                self.tail_trim
                    .set_element(element, &self.trim.element(element).now());
                v.tail_left = run.fade_samples;
                circuit = Default::default();
            }
            if trim != 0.0 {
                let mut element_trim = self.trim.element(element);
                element_trim.set(trim * accent, !circuit.settled(), run.fade_samples);
                self.trim.set_element(element, &element_trim);
            }
            circuit.fire(accent, run.shared);
            self.voice.set_element(element, &circuit);
            self.parked &= !(1 << element);
        }
        // Elements past the group's channels follow its first, so they run on
        // settings a channel could have.
        let first_controls = controls.element(0);
        for element in members.len()..L::WIDTH {
            controls.set_element(element, &first_controls);
        }

        let settled = self.voice.settled().bits() & live & !self.parked;
        if settled != 0 {
            for (element, channel) in members.iter_mut().enumerate() {
                if settled & (1 << element) != 0 {
                    A::voices(channel).voice = self.voice.element(element);
                }
            }
            self.parked |= settled;
        }

        let sounding = live & !self.parked;
        let mut y = c::<L::S>(0.0);
        if sounding != 0 {
            let step = self.voice.step(run.shared, &controls);
            y = L::S::select(L::mask(sounding), step, y);
        }
        A::group_stepped(members, &self.voice, sounding);
        if trim != 0.0 {
            y *= c::<L::S>(1.0) + self.trim.next();
        }
        let mut tails = 0;
        let mut fade = c::<L::S>(0.0);
        for (element, channel) in members.iter_mut().enumerate() {
            let v = A::voices(channel);
            if v.tail_left > 0 {
                tails |= 1 << element;
                fade.set_element(element, &(v.tail_left as f32 / run.fade_samples as f32));
                v.tail_left -= 1;
            }
        }
        if tails != 0 {
            let mut tail = fade * self.tail.step(run.shared, &controls);
            if trim != 0.0 {
                tail *= c::<L::S>(1.0) + self.tail_trim;
            }
            y = L::S::select(L::mask(tails), under(y, tail), y);
        }
        for element in 0..members.len() {
            out(first + element, y.element(element));
        }
    }
}
