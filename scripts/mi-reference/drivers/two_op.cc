// Plaits' two-operator FM engine (fm_engine.cc) with Operator's control model:
// controls set every sample instead of ramped across 12-sample blocks, exact
// pitch conversions, knobs in volts, and signed phase increments so a negative
// frequency runs backward. Each record is one sample's inputs then its `out`
// and `aux` samples, before the output gain.

#include <cmath>
#include <cstdint>
#include <cstdio>

#include "plaits/dsp/downsampler/4x_downsampler.h"
#include "plaits/dsp/oscillator/sine_oscillator.h"
#include "plaits/resources.h"
#include "stmlib/dsp/dsp.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;

static float clamp(float x, float lo, float hi) {
  return x < lo ? lo : (x > hi ? hi : x);
}

static uint32_t increment(float cycles) {
  return static_cast<uint32_t>(static_cast<int64_t>(4294967296.0f * cycles));
}

int main(int argc, char** argv) {
  uint32_t carrier_phase = 0, modulator_phase = 0, sub_phase = 0;
  float previous_sample = 0.0f, carrier_fir = 0.0f, sub_fir = 0.0f;

  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  const int samples = 9600;
  for (int n = 0; n < samples; ++n) {
    const float t = float(n) / samples;
    const int segment = n / 1600;
    float note = -2.0f + segment * 0.9f;
    if (segment == 2) note = -1.0f + 3.0f * t;
    float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    // Through-zero: the carrier sweeps down through zero and back.
    if (segment == 4) f0 = 0.01f * sinf(float(n) * 0.002f);
    float in[4] = {
        f0,
        2.5f + 2.6f * sinf(float(n) * 0.0011f),        // ratio
        2.5f + 2.6f * sinf(float(n) * 0.0023f + 1.0f), // index
        5.2f * sinf(float(n) * 0.0017f + 2.0f),        // feedback
    };

    const float ratio = Interpolate(
        lut_fm_frequency_quantizer, clamp(in[1] * 0.2f, 0.0f, 1.0f), 128.0f);
    const float carrier_frequency = f0 * 0.25f;
    const float modulator_frequency =
        clamp(carrier_frequency * exp2f(ratio / 12.0f), -0.5f, 0.5f);
    const float note_number =
        69.0f + 12.0f * log2f(fabsf(f0) * 48000.0f / 440.0f);
    float hf_taming = 1.0f - (note_number - 24.0f + ratio - 72.0f) * 0.025f;
    hf_taming = clamp(hf_taming, 0.0f, 1.0f);
    hf_taming *= hf_taming;
    const float timbre = clamp(in[2] * 0.2f, 0.0f, 1.0f);
    const float amount = 2.0f * timbre * timbre * hf_taming;
    const float feedback = clamp(in[3] * 0.2f, -1.0f, 1.0f);

    Downsampler carrier_downsampler(&carrier_fir);
    Downsampler sub_downsampler(&sub_fir);
    const float phase_feedback = feedback < 0.0f ? 0.5f * feedback * feedback : 0.0f;
    const uint32_t carrier_increment = increment(carrier_frequency);
    for (size_t j = 0; j < kOversampling; ++j) {
      modulator_phase += increment(
          modulator_frequency * (1.0f + previous_sample * phase_feedback));
      carrier_phase += carrier_increment;
      sub_phase += static_cast<uint32_t>(
          static_cast<int32_t>(carrier_increment) >> 1);
      float modulator_fb = feedback > 0.0f ? 0.25f * feedback * feedback : 0.0f;
      float modulator = SinePM(modulator_phase, modulator_fb * previous_sample);
      float carrier = SinePM(carrier_phase, amount * modulator);
      float sub = SinePM(sub_phase, amount * carrier * 0.25f);
      ONE_POLE(previous_sample, carrier, 0.05f);
      carrier_downsampler.Accumulate(j, carrier);
      sub_downsampler.Accumulate(j, sub);
    }
    float out[2] = {carrier_downsampler.Read(), sub_downsampler.Read()};

    fwrite(in, sizeof(float), 4, f);
    fwrite(out, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
