// Plaits' two snare drums with Operator's control model: each renders one
// sample at a time, so its controls are set from that sample's pitch and
// knobs, with exact pitches, knobs in volts and a per-sample trigger.
// AnalogSnareDrum runs unmodified. SyntheticSnareDrum runs as a copy of its
// class that differs in one marked place, as `$mi.synthSnare` does: Plaits
// decays the drum's quiet tail (below 0.03) on the odd samples of each
// 12-sample block, which a one-sample render cannot reproduce, so the copy
// decays it on odd samples counted from the start. Each drum draws from its
// own seed of stmlib's random generator, as each module channel has its own.
// Each record is one sample's inputs then the analog drum and the synthetic
// drum, before the output gains.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc

#include <algorithm>
#include <cmath>
#include <cstdio>

#include "plaits/dsp/drums/analog_snare_drum.h"
#include "stmlib/dsp/dsp.h"
#include "stmlib/dsp/filter.h"
#include "stmlib/dsp/parameter_interpolator.h"
#include "stmlib/dsp/units.h"
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

// plaits::SyntheticSnareDrum (synthetic_snare_drum.h), rendering one sample
// per call.
class SyntheticSnareDrum {
 public:
  void Init() {
    phase_[0] = 0.0f;
    phase_[1] = 0.0f;
    drum_amplitude_ = 0.0f;
    snare_amplitude_ = 0.0f;
    fm_ = 0.0f;
    hold_counter_ = 0;
    sustain_gain_ = 0.0f;
    odd_sample_ = false;

    drum_lp_.Init();
    snare_hp_.Init();
    snare_lp_.Init();
  }

  inline float DistortedSine(float phase) {
    float triangle = (phase < 0.5f ? phase : 1.0f - phase) * 4.0f - 1.3f;
    return 2.0f * triangle / (1.0f + fabsf(triangle));
  }

  float Render(
      bool sustain,
      bool trigger,
      float accent,
      float f0,
      float fm_amount,
      float decay,
      float snappy) {
    const float decay_xt = decay * (1.0f + decay * (decay - 1.0f));
    fm_amount *= fm_amount;
    const float drum_decay = 1.0f - 1.0f / (0.015f * kSampleRate) * \
        stmlib::SemitonesToRatio(
           -decay_xt * 72.0f - fm_amount * 12.0f + snappy * 7.0f);
    const float snare_decay = 1.0f - 1.0f / (0.01f * kSampleRate) * \
        stmlib::SemitonesToRatio(-decay * 60.0f - snappy * 7.0f);
    const float fm_decay = 1.0f - 1.0f / (0.007f * kSampleRate);

    snappy = snappy * 1.1f - 0.05f;
    CONSTRAIN(snappy, 0.0f, 1.0f);

    const float drum_level = stmlib::Sqrt(1.0f - snappy);
    const float snare_level = stmlib::Sqrt(snappy);

    const float snare_f_min = std::min(10.0f * f0, 0.5f);
    const float snare_f_max = std::min(35.0f * f0, 0.5f);

    snare_hp_.set_f<stmlib::FREQUENCY_FAST>(snare_f_min);
    snare_lp_.set_f_q<stmlib::FREQUENCY_FAST>(snare_f_max,
        0.5f + 2.0f * snappy);
    drum_lp_.set_f<stmlib::FREQUENCY_FAST>(3.0f * f0);

    if (trigger) {
      snare_amplitude_ = drum_amplitude_ = 0.3f + 0.7f * accent;
      fm_ = 1.0f;
      phase_[0] = phase_[1] = 0.0f;
      hold_counter_ = static_cast<int>((0.04f + decay * 0.03f) * kSampleRate);
    }

    stmlib::ParameterInterpolator sustain_gain(
        &sustain_gain_,
        accent * decay,
        size_t(1));
    // Deviation: the parity of the sample count from the start stands in
    // for Plaits' `!(size & 1)`.
    const bool odd = odd_sample_;
    odd_sample_ = !odd_sample_;
    if (sustain) {
      snare_amplitude_ = sustain_gain.Next();
      drum_amplitude_ = snare_amplitude_;
      fm_ = 0.0f;
    } else {
      drum_amplitude_ *= (drum_amplitude_ > 0.03f || odd)
          ? drum_decay
          : 1.0f;
      if (hold_counter_) {
        --hold_counter_;
      } else {
        snare_amplitude_ *= snare_decay;
      }
      fm_ *= fm_decay;
    }

    float reset_noise = 0.0f;
    float reset_noise_amount = (0.125f - f0) * 8.0f;
    CONSTRAIN(reset_noise_amount, 0.0f, 1.0f);
    reset_noise_amount *= reset_noise_amount;
    reset_noise_amount *= fm_amount;
    reset_noise += phase_[0] > 0.5f ? -1.0f : 1.0f;
    reset_noise += phase_[1] > 0.5f ? -1.0f : 1.0f;
    reset_noise *= reset_noise_amount * 0.025f;

    float f = f0 * (1.0f + fm_amount * (4.0f * fm_));
    phase_[0] += f;
    phase_[1] += f * 1.47f;
    if (reset_noise_amount > 0.1f) {
      if (phase_[0] >= 1.0f + reset_noise) {
        phase_[0] = 1.0f - phase_[0];
      }
      if (phase_[1] >= 1.0f + reset_noise) {
        phase_[1] = 1.0f - phase_[1];
      }
    } else {
      if (phase_[0] >= 1.0f) {
        phase_[0] -= 1.0f;
      }
      if (phase_[1] >= 1.0f) {
        phase_[1] -= 1.0f;
      }
    }

    float drum = -0.1f;
    drum += DistortedSine(phase_[0]) * 0.60f;
    drum += DistortedSine(phase_[1]) * 0.25f;
    drum *= drum_amplitude_ * drum_level;
    drum = drum_lp_.Process<stmlib::FILTER_MODE_LOW_PASS>(drum);

    float noise = stmlib::Random::GetFloat();
    float snare = snare_lp_.Process<stmlib::FILTER_MODE_LOW_PASS>(noise);
    snare = snare_hp_.Process<stmlib::FILTER_MODE_HIGH_PASS>(snare);
    snare = (snare + 0.1f) * (snare_amplitude_ + fm_) * snare_level;

    return snare + drum;
  }

 private:
  float phase_[2];
  float drum_amplitude_;
  float snare_amplitude_;
  float fm_;
  float sustain_gain_;
  int hold_counter_;
  bool odd_sample_;

  stmlib::OnePole drum_lp_;
  stmlib::OnePole snare_hp_;
  stmlib::Svf snare_lp_;
};

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

static AnalogSnareDrum analog;
static SyntheticSnareDrum synthetic;

const int kSegment = 9600;
const int kSegments = 9;

// The pitch in cycles per sample for segment `s` at sample `n`.
static float Pitch(int s, int n) {
  switch (s) {
    case 4:
      // Swept by an audio-rate sine around 200 Hz, as FM sweeps it.
      return 200.0f * float(exp2(double(0.8f * sinf(float(n) * 0.013f)))) /
             48000.0f;
    case 5:
      // Past the shell's top mode and the noise filter's cap, then near 1 Hz.
      return (n % kSegment < kSegment / 2 ? 5000.0f : 1.5f) / 48000.0f;
    default:
      return (150.0f + 20.0f * float(s)) / 48000.0f;
  }
}

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  analog.Init();
  synthetic.Init();
  uint32_t analog_seed = 0x1234567u;
  uint32_t synthetic_seed = 0x7654321u;

  for (int n = 0; n < kSegment * kSegments; ++n) {
    const int s = n / kSegment;
    const int i = n % kSegment;
    const float t = float(n);
    // Segments 0-5 strike; 6 drones with the trigger unpatched; 7 strikes
    // again; 8 strikes once and rings out.
    const bool sustain = s == 6;
    bool trigger;
    switch (s) {
      case 3:
        // Retriggered mid-decay at uneven gaps, then at audio rate.
        trigger = i < 9000 ? (i % 1373) % 457 == 0 : i % 2 == 0;
        break;
      case 8:
        trigger = i == 0;
        break;
      default:
        trigger = !sustain && i % 2400 == 0;
        break;
    }
    float in[8] = {
        Pitch(s, n),
        2.5f,  // tone
        2.5f,  // decay
        2.5f,  // snappy
        4.0f,  // accent
        2.5f,  // attack FM
        sustain ? 1.0f : 0.0f,
        trigger ? 1.0f : 0.0f,
    };
    switch (s) {
      case 0:
        // Accents across the range.
        in[4] = 2.5f + 2.6f * sinf(t * 0.0011f);
        break;
      case 1:
        // Tone across the 2/3 split and decay swept.
        in[1] = 2.5f + 2.6f * sinf(t * 0.00071f);
        in[2] = 2.5f + 2.6f * sinf(t * 0.00053f + 1.0f);
        break;
      case 2:
        // Snappy, attack FM and decay swept.
        in[3] = 2.5f + 2.6f * sinf(t * 0.00067f);
        in[5] = 2.5f + 2.6f * sinf(t * 0.00043f + 1.0f);
        in[2] = 2.5f + 2.6f * sinf(t * 0.00031f + 2.0f);
        break;
      case 3:
        in[2] = 4.0f;
        in[5] = 4.0f;
        break;
      case 4:
        in[5] = 5.0f;
        break;
      case 5:
        in[1] = 4.5f;
        in[5] = 5.0f;
        break;
      case 6:
        // The drone: decay sets its level; every knob moves.
        in[1] = 2.5f + 2.6f * sinf(t * 0.00071f);
        in[2] = 2.5f + 2.6f * sinf(t * 0.00053f + 1.0f);
        in[3] = 2.5f + 2.6f * sinf(t * 0.00043f + 2.0f);
        in[5] = 2.5f + 2.6f * sinf(t * 0.00037f + 3.0f);
        break;
      case 8:
        in[3] = 1.0f;
        break;
    }

    const float f0 = in[0];
    const float accent = knob(in[4]);
    const float decay = knob(in[2]);
    const float snappy = knob(in[3]);
    float out, aux;
    Random::Seed(analog_seed);
    analog.Render(sustain, trigger, accent, f0, knob(in[1]), decay, snappy,
                  &out, 1);
    analog_seed = Random::state();
    Random::Seed(synthetic_seed);
    aux = synthetic.Render(sustain, trigger, accent, f0, knob(in[5]), decay,
                           snappy);
    synthetic_seed = Random::state();
    float result[2] = {out, aux};
    fwrite(in, sizeof(float), 8, f);
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
