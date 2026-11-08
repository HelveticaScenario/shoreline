//! Single-band feed-forward compressor module.
//!
//! Peak-detecting compressor with configurable threshold, ratio,
//! attack/release times, makeup gain, and input/output gain staging.

use deserr::Deserr;
use schemars::JsonSchema;

use crate::dsp::utils::{cached::Cached, sanitize};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};

// Gain voltage scaling: maps [-5, 5] volts to [-24, 24] dB (4.8 dB per volt)
const DB_PER_VOLT: f32 = 4.8;

/// Convert a bipolar voltage (-5 to +5) to a linear gain multiplier.
/// 0V = 0dB (unity), -5V = -24dB, +5V = +24dB.
#[inline]
fn voltage_to_gain(voltage: f32) -> f32 {
    let db = voltage.clamp(-5.0, 5.0) * DB_PER_VOLT;
    10.0_f32.powf(db / 20.0)
}

/// Convert a threshold voltage to dBV, flooring at ~-200 dB so 0 V stays finite.
#[inline]
fn threshold_to_db(threshold: f32) -> f32 {
    20.0 * (threshold.max(0.0) + 1e-10).log10()
}

/// One-pole envelope follower coefficient for a time constant in seconds.
#[inline]
fn follower_coeff(time: f32, sample_rate: f32) -> f32 {
    (-1.0 / (time.max(1e-6) * sample_rate)).exp()
}

/// Compute compressor gain for a single sample.
///
/// Envelope is updated from `detector_sample` (the side-chain signal — for
/// internal detection this is the same as `sample`), using the
/// [`follower_coeff`]s for attack and release. Two stages run on every
/// sample: a downward stage active above `threshold_db` and an upward stage
/// active below `upward_threshold_db` (both from [`threshold_to_db`]). Each
/// stage's `ratio` is interpreted via the gain factor `1 − 1/ratio`:
///
/// - `ratio == 1` → factor 0 → passthrough.
/// - `ratio > 1` → positive factor → compression (pull toward threshold).
/// - `ratio < 1` → negative factor → expansion (push away from threshold).
///
/// To keep expansion bounded at signal extremes, `level_db` is clamped to
/// `[-60, +60]` dB before the gain math.
#[inline]
fn compress(
    sample: f32,
    detector_sample: f32,
    envelope: &mut f32,
    threshold_db: f32,
    ratio: f32,
    upward_threshold_db: f32,
    upward_ratio: f32,
    attack_coeff: f32,
    release_coeff: f32,
    makeup: f32,
) -> f32 {
    // Lower-bound the ratios at 1e-3 so `1/ratio` stays finite. The gain math
    // is shaped by `(1 - 1/ratio)`: ratio ∈ (0, 1) is expansion, ratio == 1 is
    // passthrough, ratio > 1 is compression. The ±60 dB clamp on `level_db`
    // below is what actually keeps the output bounded at extreme ratios.
    let ratio = ratio.max(1e-3);
    let upward_ratio = upward_ratio.max(1e-3);

    // Envelope follower (peak detection with attack/release ballistics)
    let detector_abs = detector_sample.abs();
    let coeff = if detector_abs > *envelope {
        attack_coeff
    } else {
        release_coeff
    };
    *envelope = detector_abs + coeff * (*envelope - detector_abs);
    *envelope = sanitize(*envelope);

    // Gain computation in dB domain. Clamp the detected level to ±60 dB so
    // expansion (negative gain factor) stays bounded at extreme inputs:
    // without the floor, silence drives upward expansion to +∞ dB; without
    // the ceiling, very loud peaks drive downward expansion the same way.
    let level_db = (20.0 * (*envelope + 1e-10).log10()).clamp(-60.0, 60.0);

    // Downward stage (above threshold): compress when ratio > 1, expand when
    // ratio < 1. Factor is 0 at ratio == 1 → passthrough, no branch needed.
    let down_db = if level_db > threshold_db {
        (threshold_db - level_db) * (1.0 - 1.0 / ratio)
    } else {
        0.0
    };

    // Upward stage (below threshold): compress (boost quiet signal) when
    // ratio > 1, expand (gate-like attenuation of quiet signal) when
    // ratio < 1.
    let up_db = if level_db < upward_threshold_db {
        (upward_threshold_db - level_db) * (1.0 - 1.0 / upward_ratio)
    } else {
        0.0
    };

    // Clamp the final gain to ±60 dB. The level clamp alone isn't enough at
    // extreme expansion ratios — factor `1 - 1/ratio` is unbounded (e.g.
    // ratio = 1e-3 → factor ≈ -1000), and even a bounded level_db produces
    // hundreds of dB of swing which overflows when raised back to linear.
    let gain_db = (down_db + up_db).clamp(-60.0, 60.0);
    let gain = 10.0_f32.powf(gain_db / 20.0);

    sample * gain * makeup
}

#[derive(Clone, Copy, Default)]
struct ChannelState {
    envelope: f32,
    input_gain: Cached<f32>,
    output_gain: Cached<f32>,
    makeup: Cached<f32>,
    threshold_db: Cached<f32>,
    upward_threshold_db: Cached<f32>,
    attack_coeff: Cached<f32>,
    release_coeff: Cached<f32>,
}

#[params]
#[derive(Clone, Deserr, JsonSchema, Connect, ChannelCount)]
#[serde(rename_all = "camelCase")]
#[deserr(rename_all = camelCase, deny_unknown_fields)]
struct CompressorParams {
    #[input("the sound to compress", type = audio)]
    input: PolySignal,
    #[input(
        "when patched, the compressor listens to this instead of the input, but still turns the input down; for ducking and pumping",
        type = audio,
    )]
    #[deserr(default)]
    sidechain: Option<PolySignal>,
    #[input(
        "level above which the signal is compressed, in volts",
        type = control,
        default = 2.5,
        range = (0.0, ..),
        control_range = (0.0, 5.0),
    )]
    #[deserr(default)]
    threshold: Option<PolySignal>,
    #[input(
        "compression ratio above the threshold, as X:1: above 1 compresses, 1 passes through, below 1 makes loud parts louder",
        type = control,
        default = 4.0,
        range = (0.001, ..),
        control_range = (1.0, 20.0),
    )]
    #[deserr(default)]
    ratio: Option<PolySignal>,
    #[input(
        "level below which upwardRatio acts, in volts; 0 turns the upward stage off",
        type = control,
        default = 0.0,
        range = (0.0, ..),
        control_range = (0.0, 5.0),
    )]
    #[deserr(default)]
    upward_threshold: Option<PolySignal>,
    #[input(
        "ratio below upwardThreshold, as X:1: above 1 lifts quiet parts, 1 passes through, below 1 gates them",
        type = control,
        default = 1.0,
        range = (0.001, ..),
        control_range = (0.1, 10.0),
    )]
    #[deserr(default)]
    upward_ratio: Option<PolySignal>,
    #[input(
        "how fast the compressor reacts to rising level, in seconds",
        type = control,
        default = 0.01,
        range = (0.0, ..),
        control_range = (0.0, 0.2),
    )]
    #[deserr(default)]
    attack: Option<PolySignal>,
    #[input(
        "how fast the compressor lets go as level falls, in seconds",
        type = control,
        default = 0.1,
        range = (0.0, ..),
        control_range = (0.0, 2.0),
    )]
    #[deserr(default)]
    release: Option<PolySignal>,
    #[input(
        "gain after compression: -5 V = -24 dB, +5 V = +24 dB",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    makeup: Option<PolySignal>,
    #[input(
        "gain into the compressor: -5 V = -24 dB, +5 V = +24 dB; more pushes harder into compression",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    input_gain: Option<PolySignal>,
    #[input(
        "final output level: -5 V = -24 dB, +5 V = +24 dB",
        type = control,
        default = 0.0,
        range = (-5.0, 5.0),
    )]
    #[deserr(default)]
    output_gain: Option<PolySignal>,
    #[input(
        "dry/wet blend: 0 dry, 5 wet; the dry signal skips inputGain",
        type = control,
        default = 5.0,
        range = (0.0, 5.0),
    )]
    #[deserr(default)]
    mix: Option<PolySignal>,
}

#[derive(Outputs, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CompressorOutputs {
    #[output("sample", "compressed signal", default)]
    sample: PolyOutput,
}

/// EXPERIMENTAL
///
/// Single-band feed-forward dynamics processor with peak envelope follower.
///
/// Two stages run in parallel:
///
/// - **downward stage** acts on signal above `threshold`. `ratio > 1` =
///   compression, `ratio < 1` = upward expansion (boost loud), `ratio = 1` =
///   passthrough.
/// - **upward stage** acts on signal below `upwardThreshold`. `upwardRatio > 1`
///   = upward compression (boost quiet), `upwardRatio < 1` = downward
///   expansion / gate-like attenuation of quiet signal, `upwardRatio = 1` =
///   passthrough.
///
/// Both ratios use textbook `X:1` notation (X = Δinput dB / Δoutput dB) and
/// are lower-bounded at 1e-3 (≈ Ableton's 1:1000 display). The detected
/// level is clamped to ±60 dB so expansion stays bounded at signal extremes.
///
/// An optional **side-chain** input lets the envelope follower track an
/// external signal while the gain is applied to `input` — useful for
/// ducking, kick-triggered pumping, or building multiband effects like OTT.
///
/// **Signal flow:** input → input gain → (detector: side-chain or input)
///   → up + down compression → output gain → dry/wet mix → output
///
/// - **threshold** — downward stage threshold in volts (0–5, default 2.5).
/// - **ratio** — downward stage ratio in textbook X:1 form. > 1 compresses,
///   < 1 expands (boosts loud signal), 1 = passthrough. Lower-bounded at
///   1e-3 (≈ 1:1000 expansion); upper bound is `f32::INFINITY` =
///   brick-wall limiter. Default 4.0 (= Ableton display 1:0.25).
/// - **upwardThreshold** — upward stage threshold in volts (0–5, default 0).
/// - **upwardRatio** — upward stage ratio in textbook X:1 form. > 1 boosts
///   quiet signal, < 1 gates quiet signal, 1 = passthrough. Same bounds as
///   `ratio`. Default 1 (passthrough).
/// - **sidechain** — optional external detector signal.
/// - **attack** / **release** — envelope follower time constants in seconds.
/// - **makeup** — post-compression makeup gain as a dB-domain voltage
///   (-5V = -24dB, 0V = unity, +5V = +24dB, default 0).
/// - **inputGain** — gain before the compressor (-5V = -24dB, 0V = unity,
///   5V = +24dB). Raising input gain drives more signal into the compressor.
/// - **outputGain** — gain after compression (-5V = -24dB, 0V = unity,
///   5V = +24dB). Trims the final output level.
/// - **mix** — dry/wet blend (0 = fully dry, 5 = fully wet, default 5.0).
///   The dry signal is the original input before any gain staging.
///
/// ```js
/// // simple bus compressor
/// $comp($saw('c2'), { threshold: 2.5, ratio: 4, attack: 0.01, release: 0.1 })
/// ```
///
/// ```js
/// // side-chain ducking — a kick envelope ducks a sustained pad
/// $comp($saw('c3'), {
///   sidechain: $perc($clock.beatTrigger, { decay: 0.2 }),
///   threshold: 1.0, ratio: 8, attack: 0.005, release: 0.2,
/// })
/// ```
///
/// ```js
/// // multiband compression using $xover + $comp
/// let bands = $xover($saw('c2'), { lowMidFreq: '200hz', midHighFreq: '2000hz' })
/// let low  = $comp(bands.low,  { threshold: 2.5, ratio: 4 })
/// let mid  = $comp(bands.mid,  { threshold: 3,   ratio: 3 })
/// let high = $comp(bands.high, { threshold: 2,   ratio: 6 })
/// $mix([low, mid, high]).out()
/// ```
#[module(name = "$comp", categories(dynamics), args(input))]
pub struct Compressor {
    outputs: CompressorOutputs,
    channel_state: Box<[ChannelState]>,
    params: CompressorParams,
}

impl Compressor {
    fn update(&mut self, sample_rate: f32) {
        let channels = self.channel_count();

        for ch in 0..channels {
            let state = &mut self.channel_state[ch];

            let input = self.params.input.get_value(ch);

            // Apply input gain
            let input_gain_voltage = self
                .params
                .input_gain
                .value_or(ch, CompressorParams::INPUT_GAIN_DEFAULT);
            let gained = input * state.input_gain.get(input_gain_voltage, voltage_to_gain);

            // Side-chain: detector reads from sidechain input if connected,
            // otherwise from the gain-staged input itself.
            let detector = match &self.params.sidechain {
                Some(sc) => sc.get_value(ch),
                None => gained,
            };

            // Read compressor parameters
            let threshold = self
                .params
                .threshold
                .value_or(ch, CompressorParams::THRESHOLD_DEFAULT);
            let ratio = self
                .params
                .ratio
                .value_or(ch, CompressorParams::RATIO_DEFAULT);
            let upward_threshold = self
                .params
                .upward_threshold
                .value_or(ch, CompressorParams::UPWARD_THRESHOLD_DEFAULT);
            let upward_ratio = self
                .params
                .upward_ratio
                .value_or(ch, CompressorParams::UPWARD_RATIO_DEFAULT);
            let attack = self
                .params
                .attack
                .value_or(ch, CompressorParams::ATTACK_DEFAULT);
            let release = self
                .params
                .release
                .value_or(ch, CompressorParams::RELEASE_DEFAULT);
            // makeup is a dB-domain voltage matching inputGain / outputGain
            // (-5 V = -24 dB, 0 V = unity, +5 V = +24 dB). voltage_to_gain
            // clamps the voltage internally.
            let makeup_voltage = self
                .params
                .makeup
                .value_or(ch, CompressorParams::MAKEUP_DEFAULT);
            let makeup = state.makeup.get(makeup_voltage, voltage_to_gain);

            // Compress
            let compressed = compress(
                gained,
                detector,
                &mut state.envelope,
                state.threshold_db.get(threshold, threshold_to_db),
                ratio,
                state
                    .upward_threshold_db
                    .get(upward_threshold, threshold_to_db),
                upward_ratio,
                state
                    .attack_coeff
                    .get(attack, |t| follower_coeff(t, sample_rate)),
                state
                    .release_coeff
                    .get(release, |t| follower_coeff(t, sample_rate)),
                makeup,
            );

            // Apply output gain
            let output_gain_voltage = self
                .params
                .output_gain
                .value_or(ch, CompressorParams::OUTPUT_GAIN_DEFAULT);
            let out = compressed * state.output_gain.get(output_gain_voltage, voltage_to_gain);

            // Dry/wet mix (dry signal is original input before gain staging)
            let mix_amount = self
                .params
                .mix
                .value_or(ch, CompressorParams::MIX_DEFAULT)
                .clamp(0.0, 5.0)
                / 5.0;
            let output = input * (1.0 - mix_amount) + out * mix_amount;

            self.outputs.sample.set(ch, output);
        }
    }
}

message_handlers!(impl Compressor {});

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive `compress()` to steady state on a constant DC level so the
    /// envelope follower fully tracks the input, then assert the output gain.
    fn steady_state_output(
        sample: f32,
        threshold: f32,
        ratio: f32,
        upward_threshold: f32,
        upward_ratio: f32,
    ) -> f32 {
        let mut env = sample.abs();
        let mut last = 0.0;
        // Long enough run to settle the one-pole follower at any sane attack.
        for _ in 0..2_000 {
            last = compress(
                sample,
                sample,
                &mut env,
                threshold_to_db(threshold),
                ratio,
                threshold_to_db(upward_threshold),
                upward_ratio,
                follower_coeff(0.001, 48_000.0),
                follower_coeff(0.001, 48_000.0),
                1.0,
            );
        }
        last
    }

    #[test]
    fn ratio_one_is_passthrough() {
        let out = steady_state_output(2.0, 1.0, 1.0, 0.0, 1.0);
        assert!((out - 2.0).abs() < 1e-3, "got {}", out);
    }

    #[test]
    fn standard_4to1_compression() {
        // Threshold 1V (= 0 dBV). Input 2V (= ~6 dBV) → 6 dB above. At 4:1 →
        // 1.5 dB above threshold = 10^(1.5/20) ≈ 1.189 V.
        let out = steady_state_output(2.0, 1.0, 4.0, 0.0, 1.0);
        let expected = 10f32.powf(1.5 / 20.0);
        assert!(
            (out - expected).abs() < 0.01,
            "got {}, expected {}",
            out,
            expected
        );
    }

    #[test]
    fn infinite_ratio_acts_as_brick_wall_limiter() {
        // At ratio = INFINITY, anything above threshold should be pinned to
        // the threshold level.
        let out = steady_state_output(5.0, 1.0, f32::INFINITY, 0.0, 1.0);
        assert!((out - 1.0).abs() < 0.01, "got {}", out);
    }

    #[test]
    fn ratio_below_one_expands_above_threshold() {
        // ratio = 0.5 (= 1:2 expansion): input 2V (= 6 dB above 1V threshold)
        // should be boosted by another 6 dB → ~4V output.
        let out = steady_state_output(2.0, 1.0, 0.5, 0.0, 1.0);
        assert!(out > 3.5 && out < 4.5, "got {}", out);
    }

    #[test]
    fn upward_compression_boosts_quiet() {
        // Quiet signal (0.1V = -20 dBV), upward threshold 1V (0 dBV), 4:1
        // upward → 0.1V should be boosted toward 1V.
        let quiet = steady_state_output(0.1, 5.0, 1.0, 1.0, 4.0);
        assert!(quiet > 0.1, "expected boost above 0.1, got {}", quiet);
        assert!(quiet < 1.0, "should not exceed threshold, got {}", quiet);
    }

    #[test]
    fn extreme_inputs_stay_finite() {
        // Sanity: pathological combinations shouldn't NaN or blow up to inf.
        for sample in [0.0_f32, 1e-10, 0.5, 5.0, 100.0] {
            for ratio in [1e-3_f32, 0.5, 1.0, 4.0, 1e6, f32::INFINITY] {
                let out = steady_state_output(sample, 1.0, ratio, 0.0, 1.0);
                assert!(
                    out.is_finite(),
                    "non-finite output for sample={} ratio={}: {}",
                    sample,
                    ratio,
                    out
                );
            }
        }
    }
}
