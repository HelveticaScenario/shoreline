use std::ops::Range;

use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::{
        oscillators::{FmMode, apply_fm, sync_blep, sync_edge_fraction},
        utils::{SchmittTrigger, sanitize, wrap_phase},
    },
    poly::{PORT_MAX_CHANNELS, PolyOutput, PolySignal, PolySignalExt},
    types::SwapGlide,
};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount, BlockInputs)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase)]
#[deserr(deny_unknown_fields)]
struct PulseOscillatorParams {
    #[input("oscillator pitch", type = pitch)]
    freq: PolySignal,
    #[input(
        "pulse width: 0 a thin high spike, 2.5 square, 5 a thin low notch",
        type = control,
        default = 2.5,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    width: Option<PolySignal>,
    #[input(
        "pulse width modulation, added to `width`",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    pwm: Option<PolySignal>,
    #[input("frequency modulation: each volt adds the base frequency again (lin, throughZero) or an octave (exp)", type = audio)]
    #[deserr(default)]
    fm: Option<PolySignal>,
    #[input("how `fm` bends the frequency")]
    #[serde(default)]
    #[deserr(default)]
    fm_mode: FmMode,
    #[input("hard sync: each rising edge restarts the cycle", type = trig)]
    #[deserr(default)]
    #[block_input(optional)]
    sync: Option<PolySignal>,
    #[input(
        "shifts where in the cycle the wave is read, in cycles; wraps around",
        type = control,
        default = 0.0,
        range = (0.0, 1.0),
    )]
    #[deserr(default)]
    phase_offset: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PulseOscillatorOutputs {
    #[output("output", "the pulse wave", type = cv, default, range = (-5.0, 5.0))]
    sample: PolyOutput,
}

#[derive(Default, Clone, Copy)]
struct PulseChannelState {
    phase: f32,
    width: SwapGlide,
    pwm: SwapGlide,
    /// Edge detector for the sync input.
    sync_schmitt: SchmittTrigger,
    /// Previous sync-input sample, for subsample edge interpolation.
    sync_prev: f32,
    /// PolyBLEP residual carried into the next sample from a sync reset.
    blep_carry: f32,
}

const PULSE_DOCS: ModuleDocs = ModuleDocs {
    summary: "Pulse wave oscillator",
    description: "A pulse wave that is hollow and woody as a square and grows \
                  nasal and reedy as the pulse narrows. Sweeping the width with \
                  a slow LFO gives the classic chorused PWM string sound.",
    categories: &[ModuleCategory::Oscillator],
    sections: &[],
    examples: &[
        DocExample {
            title: "PWM chord pad",
            code: r#"
                $pulse($chord('C3[m]'), {
                  pwm: $sine('0.3hz').range(-2, 2),
                })
                  .lpf('c6')
                  .out()
            "#,
        },
        DocExample {
            title: "Plucked square bass",
            code: r#"
                const seq = $p('c2 c2 eb2 g1').c
                $pulse(seq)
                  .lpf($perc(seq, { decay: 0.2 }).range('c3', 'c6'), 2)
                  .out()
            "#,
        },
    ],
};

#[module(name = "$pulse", docs = PULSE_DOCS, args(freq), block)]
pub struct PulseOscillator {
    outputs: PulseOscillatorOutputs,
    channel_state: Box<[PulseChannelState]>,
    params: PulseOscillatorParams,
}

impl PulseChannelState {
    /// Advance the phase by `phase_increment` (cycles per sample) and return
    /// the next output sample. `width` is this sample's glided width plus
    /// PWM, clamped to 0–5, and `sync` its sync input, if connected.
    #[inline(always)]
    fn step(&mut self, phase_increment: f32, width: f32, offset: f32, sync: Option<f32>) -> f32 {
        // Pulse width (0.0 to 1.0, 0.5 is square wave)
        let pulse_width = (width / 5.0).clamp(0.01, 0.99);

        self.phase = wrap_phase(self.phase + phase_increment);

        // Phase offset shifts the read position without altering the
        // accumulator, so it never drifts.
        let read_offset = offset.rem_euclid(1.0);
        let read_phase = (self.phase + read_offset).rem_euclid(1.0);

        let naive_pulse = |p: f32| if p < pulse_width { 1.0 } else { -1.0 };

        // Naive pulse plus PolyBLEP at its own rising (phase 0) and falling
        // (phase = width) edges. The sync reset lands in the upcoming
        // interval, so these operate on the real, pre-reset phase.
        let abs_phase_inc = phase_increment.abs();
        let mut body = naive_pulse(read_phase);
        body += poly_blep_pulse(read_phase, abs_phase_inc);
        body -= poly_blep_pulse(
            if read_phase >= pulse_width {
                read_phase - pulse_width
            } else {
                read_phase - pulse_width + 1.0
            },
            abs_phase_inc,
        );

        let pending = self.blep_carry;
        self.blep_carry = 0.0;

        // Hard sync: a rising edge resets the phase, with a PolyBLEP placed
        // at the subsample crossing to band-limit the reset discontinuity.
        // (The jump is zero when the pulse was already high at the reset.)
        let mut now = 0.0;
        if let Some(v) = sync {
            if self.sync_schmitt.process(v) {
                let frac = sync_edge_fraction(self.sync_prev, v);
                let before = naive_pulse(read_phase);
                self.phase = 0.0;
                let after = naive_pulse(read_offset);
                let (n, carry) = sync_blep(after - before, frac);
                now = n;
                self.blep_carry = carry;
            }
            self.sync_prev = v;
        }

        (body + pending + now) * 5.0
    }
}

impl PulseOscillator {
    /// Outside a glide, reads width and PWM raw, exactly as `process_block`
    /// does.
    fn update(&mut self, sample_rate: f32) {
        let p = &self.params;
        for (ch, state) in self.channel_state.iter_mut().enumerate() {
            let width = state.width.follow(
                p.width.value_or(ch, PulseOscillatorParams::WIDTH_DEFAULT),
                p.width.glide_mark(ch),
                sample_rate,
            );
            let pwm = state.pwm.follow(
                p.pwm.value_or(ch, PulseOscillatorParams::PWM_DEFAULT),
                p.pwm.glide_mark(ch),
                sample_rate,
            );
            let pitch = p.freq.get_value(ch);
            let fm = p.fm.value_or(ch, 0.0);
            let increment = apply_fm(pitch, fm, p.fm_mode) / sample_rate;
            let offset = p
                .phase_offset
                .value_or(ch, PulseOscillatorParams::PHASE_OFFSET_DEFAULT);
            let sync = p.sync.as_ref().map(|sync| sync.get_value(ch));
            let width = (width + pwm).clamp(0.0, 5.0);
            self.outputs
                .sample
                .set(ch, state.step(increment, width, offset, sync));
        }
    }

    fn process_block(
        &mut self,
        outputs: &mut PulseOscillatorBlockOutputs,
        range: Range<usize>,
        sample_rate: f32,
    ) -> Option<()> {
        let channels = self.channel_count();
        let p = &self.params;
        let gliding = self.channel_state[..channels]
            .iter()
            .enumerate()
            .any(|(ch, state)| {
                state.width.will_glide(p.width.glide_mark(ch))
                    || state.pwm.will_glide(p.pwm.glide_mark(ch))
            });
        if gliding {
            return None;
        }
        let mut scratch = Default::default();
        let PulseOscillatorParamsBlockInputs {
            freq,
            width,
            pwm,
            fm,
            sync,
            phase_offset: offset,
        } = p.block_inputs(range.clone(), channels, &mut scratch)?;
        // A pitch held for the whole range needs its exponential only once.
        let mut held_increments = [0.0f32; PORT_MAX_CHANNELS];
        let held = freq.is_constant() && fm.is_constant();
        if held {
            let (freq, fm) = (freq.row(range.start), fm.row(range.start));
            for ((increment, &pitch), &fm) in held_increments.iter_mut().zip(freq).zip(fm) {
                *increment = apply_fm(pitch, fm, p.fm_mode) / sample_rate;
            }
        }
        let last = range.end - 1;
        for i in range {
            let (pitch, fm, offset) = (freq.row(i), fm.row(i), offset.row(i));
            let (width, pwm) = (width.row(i), pwm.row(i));
            let sync = sync.as_ref().map(|sync| sync.row(i));
            let slots = outputs
                .sample
                .slot_mut(i)
                .iter_mut()
                .zip(self.channel_state.iter_mut());
            for (ch, (out, state)) in slots.enumerate() {
                let increment = if held {
                    held_increments[ch]
                } else {
                    apply_fm(pitch[ch], fm[ch], p.fm_mode) / sample_rate
                };
                let width = (width[ch] + pwm[ch]).clamp(0.0, 5.0);
                *out =
                    sanitize(state.step(increment, width, offset[ch], sync.map(|sync| sync[ch])));
            }
        }
        let (width, pwm) = (width.row(last), pwm.row(last));
        for (ch, state) in self.channel_state[..channels].iter_mut().enumerate() {
            state.width.settle(width[ch], p.width.glide_mark(ch));
            state.pwm.settle(pwm[ch], p.pwm.glide_mark(ch));
        }
        Some(())
    }
}

// PolyBLEP for pulse wave
fn poly_blep_pulse(phase: f32, phase_increment: f32) -> f32 {
    // Detect discontinuity at phase wrap (0.0)
    if phase < phase_increment {
        let t = phase / phase_increment;
        return t + t - t * t - 1.0;
    }
    // Detect discontinuity approaching 1.0
    else if phase > 1.0 - phase_increment {
        let t = (phase - 1.0) / phase_increment;
        return t * t + t + t + 1.0;
    }
    0.0
}

message_handlers!(impl PulseOscillator {});
