use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{Buffer, PolySignal, poly::PolyOutput};

fn delay_read_derive_channel_count(params: &DelayReadParams) -> usize {
    params.buffer.channel_count().max(params.time.channels())
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct DelayReadParams {
    #[input("buffer to read, from a `$buffer`")]
    buffer: Buffer,
    #[input(
        "how far behind the newest recorded sample to read, in seconds; capped at the buffer's length",
        type = control,
        default = 0.1,
        range = (0.0, ..),
        control_range = (0.0, 5.0),
    )]
    time: PolySignal,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct DelayReadOutputs {
    #[output("output", "the delayed signal", type = cv, default)]
    sample: PolyOutput,
}

const DELAY_READ_DOCS: ModuleDocs = ModuleDocs {
    summary: "Delay tap on a buffer",
    description: "Plays a `$buffer` back a set time behind what it is recording: one \
                  tap of a delay line. Several taps on one buffer make multi-tap \
                  echoes, and a modulated time gives chorus and flanging. `$delay` \
                  builds a complete feedback delay from these parts.",
    categories: &[ModuleCategory::Effect, ModuleCategory::Utility],
    sections: &[],
    examples: &[
        DocExample {
            title: "Two-tap echo",
            code: r#"
                const voice = $saw($p('c3 eb3 g3 ~')).amp(
                  $perc($clock._beatTrigger, { decay: 0.15 }),
                )
                voice.out()
                const echoes = $buffer(voice, 1)
                $delayRead(echoes, 0.375).amp(2.5).out()
                $delayRead(echoes, 0.75).amp(1.25).out()
            "#,
        },
        DocExample {
            title: "Chorused chord",
            code: r#"
                const voice = $saw($chord('C3[m7]'))
                voice.out()
                $delayRead(
                  $buffer(voice, 0.1),
                  $sine('0.3hz').range(0.01, 0.02),
                ).out()
            "#,
        },
    ],
};

#[module(name = "$delayRead", docs = DELAY_READ_DOCS, channels_derive = delay_read_derive_channel_count, args(buffer, time))]
pub struct DelayRead {
    outputs: DelayReadOutputs,
    params: DelayReadParams,
}

impl DelayRead {
    fn update(&mut self, sample_rate: f32) {
        // Drive the source up through this reader's current slot so the
        // writer's per-slot cursor lands at the same position the reader
        // is about to read. Calling `ensure_processed()` (full block)
        // breaks feedback cycles: the writer races ahead of this reader's
        // interleave and pulls stale `block_outputs` slots back via the
        // 1-sample-delay reentrancy path.
        let slot = self.current_block_index();
        self.params.buffer.ensure_source_updated_to(slot + 1);

        let frame_count = self.params.buffer.frame_count();
        let channels = self.channel_count();
        // This slot's write position is the base the source's `tick_buffers`
        // set at block start plus `slot`. Inside a feedback cycle the source
        // may still be computing this slot, so that frame holds data from a
        // full buffer ago and the newest written frame is the one before it.
        let unwritten = usize::from(!self.params.buffer.source_wrote_slot(slot));
        let newest = (self.params.buffer.read_write_index() + slot + frame_count)
            .wrapping_sub(unwritten)
            % frame_count.max(1);

        for channel in 0..channels {
            let delay_time_secs = (self.params.time.get_value(channel) as f64).max(0.0);
            // The longest delay is `frame_count - 1` frames whichever cycle
            // member runs first; with `unwritten`, `back` counts from the
            // frame before the write position.
            let delay_frames =
                (delay_time_secs * (sample_rate as f64)).min(frame_count.saturating_sub(1) as f64);
            let back = (delay_frames - unwritten as f64).max(0.0);

            let buf_channels = self.params.buffer.channel_count().max(1);
            let buf_channel = channel % buf_channels;
            let value = self
                .params
                .buffer
                .read_hermite_behind(buf_channel, newest, back as f32);
            self.outputs.sample.set(channel, value);
        }
    }
}

message_handlers!(impl DelayRead {});
