use deserr::Deserr;
use schemars::JsonSchema;

use crate::module_category::ModuleCategory;
use crate::module_docs::{DocExample, ModuleDocs};
use crate::{
    dsp::filters::resonant_svf::{CutoffResonance, hand_over, params_mono},
    dsp::utils::{changed, sanitize, voct_to_hz},
    poly::{PolyOutput, PolySignal},
};

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct Jup6fParams {
    #[input("the sound to filter", type = audio)]
    input: PolySignal,
    #[input(
        "cutoff frequency; tops out just under half the sample rate",
        type = frequency,
        range = (-3.71, ..),
    )]
    cutoff: PolySignal,
    #[input(
        "filter resonance; self-oscillates at 5",
        type = control,
        default = 0.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    resonance: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Jup6fOutputs {
    #[output("lp24", "4-pole (24 dB/oct) lowpass", type = cv, default, range = (-5.0, 5.0))]
    lp24: PolyOutput,
    #[output("lp12", "2-pole (12 dB/oct) lowpass", type = cv, range = (-5.0, 5.0))]
    lp12: PolyOutput,
    #[output("bp", "bandpass", type = cv, range = (-5.0, 5.0))]
    bp: PolyOutput,
    #[output("hp", "highpass", type = cv, range = (-5.0, 5.0))]
    hp: PolyOutput,
}

/// 4-stage OTA ladder state for a single channel.
#[derive(Default, Clone, Copy)]
struct LadderState {
    /// Four cascaded 1-pole stages
    s: [f32; 4],
    /// Cached cutoff for change detection
    last_cutoff: f32,
    /// Cached resonance for change detection
    last_resonance: f32,
    glides: CutoffResonance,
    /// Cached tuning coefficient (g)
    g: f32,
    /// Cached resonance coefficient (k)
    k: f32,
}

const JUP6F_DOCS: ModuleDocs = ModuleDocs {
    summary: "Jupiter-6 style ladder filter",
    description: "A warm, saturating 4-pole lowpass modelled on the Roland Jupiter-6, with \
                  extra outputs for a 12 dB lowpass, a bandpass and a highpass from the same core. Turned up, \
                  the resonance self-oscillates into a clean sine at the cutoff.",
    categories: &[ModuleCategory::Filter],
    sections: &[],
    examples: &[
        DocExample {
            title: "Breathing analog pad",
            code: r#"
                $saw($chord('C3[maj9]'))
                  .jup6f($sine('0.1hz').range('c4', 'c7'), 2.5)
                  .out()
            "#,
        },
        DocExample {
            title: "Squelchy bandpass bass",
            code: r#"
                const seq = $p('c2 [c2 c3] eb2 [g1 bb1]').c
                $jup6f(
                  $saw(seq),
                  $perc(seq, { decay: 0.15 }).range('c3', 'c6'),
                  4,
                ).o.bp.out()
            "#,
        },
    ],
};

#[module(
    name = "$jup6f",
    docs = JUP6F_DOCS,
    patch_update,
    args(input, cutoff, resonance)
)]
pub struct Jup6f {
    outputs: Jup6fOutputs,
    state: Jup6fState,
    channel_state: Box<[LadderState]>,
    params: Jup6fParams,
}

/// State for the Jup6f module.
struct Jup6fState {
    /// Mono optimization
    mono_g: f32,
    mono_k: f32,
    last_cutoff_mono: f32,
    last_resonance_mono: f32,
    glides_mono: CutoffResonance,
    /// Whether the current params use the mono coefficients.
    mono_path: bool,
}

impl Default for Jup6fState {
    fn default() -> Self {
        Self {
            mono_g: 0.0,
            mono_k: 0.0,
            last_cutoff_mono: f32::NAN, // Indicate that coefficients have never been calculated
            last_resonance_mono: f32::NAN, // Indicate that coefficients have never been calculated
            glides_mono: CutoffResonance::default(),
            mono_path: false,
        }
    }
}

/// Compute the ladder tuning coefficient (g) and resonance feedback (k)
/// from V/Oct cutoff and resonance parameters.
///
/// Uses the bilinear pre-warped cutoff for accurate tuning at high frequencies.
/// The resonance is mapped from the user-facing 0–5 range to a feedback coefficient
/// where 4.0 corresponds to the theoretical self-oscillation threshold.
#[inline]
fn compute_coeffs(cutoff: f32, resonance: f32, sample_rate: f32) -> (f32, f32) {
    let freq = voct_to_hz(cutoff);
    let freq = freq.min(sample_rate * 0.45).max(20.0);

    // Bilinear pre-warp for accurate tuning
    let wc = std::f32::consts::PI * freq / sample_rate;
    let g = wc.tan();

    // Map resonance 0–5 → k 0–4.0 (self-oscillation at k=4)
    let k = (resonance / 5.0 * 4.0).max(0.0).min(4.0);

    (g, k)
}

/// Process one sample through the 4-pole OTA ladder.
///
/// Returns (lp24, lp12, bp, hp) simultaneously from the ladder taps.
///
/// The algorithm uses a resolving feedback approach (no delay-free loop issues):
/// 1. Estimate the feedback signal from the previous state
/// 2. Process through 4 cascaded 1-pole sections with tanh saturation
/// 3. Derive multimode outputs from the stage taps
#[inline]
fn process_ladder(input: f32, state: &mut [f32; 4], g: f32, k: f32) -> (f32, f32, f32, f32) {
    // Feedback from 4th stage (previous sample's output — avoids delay-free loop)
    let feedback = state[3];

    // Input with resonance feedback, saturated through tanh
    let u = input - k * feedback;

    // Coefficient for the 1-pole integrator: g / (1 + g)
    let g1 = g / (1.0 + g);

    // Stage 1
    let v0 = (u - state[0]) * g1;
    let s0 = v0 + state[0];
    state[0] = sanitize(s0 + v0);

    // Stage 2
    let v1 = (tanh_approx(s0) - state[1]) * g1;
    let s1 = v1 + state[1];
    state[1] = sanitize(s1 + v1);

    // Stage 3
    let v2 = (tanh_approx(s1) - state[2]) * g1;
    let s2 = v2 + state[2];
    state[2] = sanitize(s2 + v2);

    // Stage 4
    let v3 = (tanh_approx(s2) - state[3]) * g1;
    let s3 = v3 + state[3];
    state[3] = sanitize(s3 + v3);

    // Multimode outputs derived from ladder taps:
    // LP24 = 4th stage output (24 dB/oct)
    let lp24 = s3;
    // LP12 = 2nd stage output (12 dB/oct)
    let lp12 = s1;
    // BP = difference of LP12 and LP24 (bandpass character)
    let bp = s1 - s3;
    // HP = input minus LP24 (complementary highpass)
    let hp = tanh_approx(u) - s3;

    (lp24, lp12, bp, hp)
}

/// Fast tanh approximation.
/// Pade approximant — accurate within ~0.1% for |x| < 4, gracefully saturates beyond.
/// Zero allocations, pure arithmetic.
#[inline]
fn tanh_approx(x: f32) -> f32 {
    // Clamp to avoid overflow in x*x
    let x = x.max(-4.0).min(4.0);
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

impl Jup6f {
    fn update(&mut self, sample_rate: f32) {
        let num_channels = self.channel_count();
        let state = &mut self.state;
        let is_mono = state.mono_path;
        let channel_state = &mut self.channel_state;
        let (cutoff, resonance) = (&self.params.cutoff, &self.params.resonance);

        // Update coefficients
        if is_mono {
            let (c, r) = state
                .glides_mono
                .update(cutoff, resonance, 0.0, 0, sample_rate);

            if changed(c, state.last_cutoff_mono) || changed(r, state.last_resonance_mono) {
                let (g, k) = compute_coeffs(c, r, sample_rate);
                state.mono_g = g;
                state.mono_k = k;
                state.last_cutoff_mono = c;
                state.last_resonance_mono = r;
            }
        } else {
            for i in 0..num_channels {
                let (c, r) = channel_state[i]
                    .glides
                    .update(cutoff, resonance, 0.0, i, sample_rate);

                if changed(c, channel_state[i].last_cutoff)
                    || changed(r, channel_state[i].last_resonance)
                {
                    let (g, k) = compute_coeffs(c, r, sample_rate);
                    channel_state[i].g = g;
                    channel_state[i].k = k;
                    channel_state[i].last_cutoff = c;
                    channel_state[i].last_resonance = r;
                }
            }
        }

        for i in 0..num_channels {
            let input = self.params.input.get_value(i) * 0.2; // ±5V → ±1V

            let (g, k) = if is_mono {
                (state.mono_g, state.mono_k)
            } else {
                (channel_state[i].g, channel_state[i].k)
            };

            let (lp24, lp12, bp, hp) = process_ladder(input, &mut channel_state[i].s, g, k);

            self.outputs.lp24.set(i, lp24 * 5.0);
            self.outputs.lp12.set(i, lp12 * 5.0);
            self.outputs.bp.set(i, bp * 5.0);
            self.outputs.hp.set(i, hp * 5.0);
        }
    }
}

impl crate::types::PatchUpdateHandler for Jup6f {
    fn on_patch_update(&mut self) {
        let mono = params_mono(&self.params.cutoff, &self.params.resonance);
        hand_over(
            mono,
            self.state.mono_path,
            &mut self.state.glides_mono,
            self.channel_state.iter_mut().map(|cs| &mut cs.glides),
        );
        self.state.mono_path = mono;
    }
}

message_handlers!(impl Jup6f {});
