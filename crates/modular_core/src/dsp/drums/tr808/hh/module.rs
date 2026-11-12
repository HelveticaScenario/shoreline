use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use super::HatShared;
use super::sides::{Channel, ClosedHat, ClosedSide, OpenHat, OpenSide};
use crate::dsp::drums::group::{Groups, Run};
use crate::dsp::drums::input::{Held, Hold, Input, Live, each_slot_of};
use crate::dsp::drums::tr808::metal::MetalFilters;
use crate::dsp::drums::voice::rails;
use crate::dsp::utils::rng::seed_base;
use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::poly::{PORT_MAX_CHANNELS, PolyOutput, PolySignal};

/// Scale each hat's output so an unaccented hit peaks near 5 V.
const CLOSED_GAIN: f32 = 2.996;
const OPEN_GAIN: f32 = 2.101;

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct Hh808Params {
    #[input(
        "rising edges strike the closed hat, which also cuts off a ringing open hat",
        type = trig,
    )]
    closed: PolySignal,
    #[input("rising edges strike the open hat", type = trig)]
    #[deserr(default)]
    open: Option<PolySignal>,
    #[input(
        "how hard the hit lands, read at each strike of either hat: up to twice as loud at 5 V",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    accent: Option<PolySignal>,
    #[input(
        "how long the open hat rings",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    decay: Option<PolySignal>,
    #[input(
        "how hard a closed hit cuts off the open hat: -5 lets it ring on, 0 is the 808's own cut of about 100 ms, 5 silences it in a few milliseconds",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    cut: Option<PolySignal>,
    #[input(
        "start every hit from rest, fading out one still sounding on the same hat over 2 ms; off, each hit builds on the last, as on the 808"
    )]
    #[serde(default)]
    #[deserr(default)]
    restart: bool,
    #[input(
        "sample the controls on each strike of either hat and hold them until the next, as accent always is, so changes reach only the hits after them"
    )]
    #[serde(default)]
    #[deserr(default)]
    hold: bool,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Hh808Outputs {
    #[output("both", "both hats; an unaccented hit peaks near ±5 V", type = cv, default, range = (-10.0, 10.0))]
    sample: PolyOutput,
    #[output("closed", "the closed hat alone", type = cv, range = (-10.0, 10.0))]
    closed: PolyOutput,
    #[output("open", "the open hat alone", type = cv, range = (-10.0, 10.0))]
    open: PolyOutput,
}

const HH808_DOCS: ModuleDocs = ModuleDocs {
    summary: "TR-808 hi-hats",
    description: "The 808's closed and open hi-hats: the cymbal's square waves filtered \
                  to a thin, bright band, short and tight when closed and ringing when \
                  open. As on the 808, a closed hit chokes a ringing open hat.",
    categories: &[ModuleCategory::Drum],
    sections: &[],
    examples: &[DocExample {
        title: "Hats with open offbeats",
        code: r#"
            $tr808.kick($p('c*4')).out()
            const open = $p('~ c ~ c ~ c ~ c')
            $tr808.hiHat($p('c ~ c ~ c ~ c ~'), { open }).out()
        "#,
    }],
};

#[module(
    name = "$tr808.hiHat",
    docs = HH808_DOCS,
    has_init,
    patch_update,
    args(closed),
    block
)]
pub struct Hh808 {
    outputs: Hh808Outputs,
    params: Hh808Params,
    state: HhState,
    channel_state: Box<[Channel]>,
}

#[derive(Default)]
struct HhState {
    hold: Hold<2, 2>,
    shared: HatShared,
    filters: MetalFilters,
    closed: Groups<ClosedHat>,
    open: Groups<OpenHat>,
}

impl Hh808 {
    fn init(&mut self, sample_rate: f32) {
        self.state.shared = HatShared::new(sample_rate);
        self.state.filters = MetalFilters::new(sample_rate);
        self.state.closed = Groups::new();
        self.state.open = Groups::new();
    }

    fn update(&mut self, _sample_rate: f32) {
        let channels = self.channel_count();
        let p = &self.params;
        let inputs = [
            Live::new(&p.closed),
            Live::or(&p.open, Hh808Params::OPEN_DEFAULT),
            Live::or(&p.accent, Hh808Params::ACCENT_DEFAULT),
            Live::or(&p.decay, Hh808Params::DECAY_DEFAULT),
            Live::or(&p.cut, Hh808Params::CUT_DEFAULT),
        ];
        let o = &mut self.outputs;
        self.state.tick(
            &mut self.channel_state[..channels],
            p.restart,
            p.hold,
            inputs,
            |ch, closed, open| {
                o.closed.set(ch, rails(closed));
                o.open.set(ch, rails(open));
                o.sample.set(ch, rails(closed + open));
            },
        );
    }

    fn process_block(
        &mut self,
        outputs: &mut Hh808BlockOutputs,
        range: Range<usize>,
        _sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let p = &self.params;
        let mut scratch = Default::default();
        let Hh808ParamsBlockInputs {
            closed,
            open,
            accent,
            decay,
            cut,
        } = p.block_inputs(range.clone(), channels, &mut scratch)?;
        let channel_state = &mut self.channel_state[..channels];
        each_slot_of(
            [&mut outputs.sample, &mut outputs.closed, &mut outputs.open],
            range,
            [closed, open, accent, decay, cut],
            |inputs, [mut both, mut closed, mut open]| {
                self.state
                    .tick(channel_state, p.restart, p.hold, inputs, |ch, c, o| {
                        closed.set(ch, rails(c));
                        open.set(ch, rails(o));
                        both.set(ch, rails(c + o));
                    })
            },
        );
        Some(())
    }
}

impl HhState {
    /// Runs one sample of every channel on its `[closed, open, accent,
    /// decay, cut]`, handing `out` each one's closed and open hats.
    #[inline(always)]
    fn tick<I: Input>(
        &mut self,
        channels: &mut [Channel],
        restart: bool,
        hold: bool,
        [closed, open, accent, decay, cut]: [I; 5],
        mut out: impl FnMut(usize, f32, f32),
    ) {
        let [decay, cut] = self
            .hold
            .apply(hold, channels.len(), [closed, open], [decay, cut]);
        let (closed, open) = (Held::Live(closed), Held::Live(open));
        let mut closed_out = [0.0; PORT_MAX_CHANNELS];
        let side = ClosedSide {
            trigger: closed,
            cut,
            filters: &self.filters,
        };
        let run = Run {
            accent,
            restart,
            fade_samples: self.shared.timing.fade_samples,
            shared: &self.shared,
        };
        self.closed.run(&side, channels, &run, |ch, v| {
            closed_out[ch] = v * CLOSED_GAIN
        });
        let side = OpenSide {
            trigger: open,
            decay,
        };
        let run = Run {
            accent,
            restart,
            fade_samples: self.shared.timing.fade_samples,
            shared: &self.shared,
        };
        self.open.run(&side, channels, &run, |ch, v| {
            out(ch, closed_out[ch], v * OPEN_GAIN)
        });
    }
}

impl crate::types::PatchUpdateHandler for Hh808 {
    fn on_patch_update(&mut self) {
        let base = seed_base(self);
        for (ch, cs) in self.channel_state.iter_mut().enumerate() {
            cs.seed(base, ch);
        }
        let count = self.channel_count();
        self.state
            .closed
            .layout::<ClosedSide<&[f32]>>(&mut self.channel_state, count);
        self.state
            .open
            .layout::<OpenSide<&[f32]>>(&mut self.channel_state, count);
    }
}

message_handlers!(impl Hh808 {});
