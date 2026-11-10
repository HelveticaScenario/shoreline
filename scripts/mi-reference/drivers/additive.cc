// Plaits' additive engine (additive_engine.cc + HarmonicOscillator) with
// Operator's control model: the amplitudes update on Plaits' 12-sample
// timebase from the knobs' mean since the last update and ramp across it, and
// the oscillators render one sample at a time from that sample's pitch and
// ramped amplitudes. Each record is one sample's inputs then the integer
// series' and the organ series' samples, before the output gain.
// build: stmlib/dsp/units.cc plaits/dsp/engine/additive_engine.cc

#include <cmath>
#include <cstdio>

#define private public
#include "plaits/dsp/engine/additive_engine.h"
#undef private

using namespace plaits;

const float kC4 = 261.625565f;
const int kBlock = 12;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

static const int integer_indices[24] = {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23};
static const int organ_indices[8] = {0, 1, 2, 3, 5, 7, 9, 11};

int main(int argc, char** argv) {
  static char memory[4096];
  stmlib::BufferAllocator allocator(memory, sizeof(memory));
  AdditiveEngine engine;
  engine.Init(&allocator);
  engine.Reset();

  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }

  // The ramps' values and per-sample increments: 24 integer, 12 organ.
  float value[36] = {0.0f};
  float increment[36] = {0.0f};
  float sum[3] = {0.0f, 0.0f, 0.0f};
  int count = 0;

  const int samples = 14400;
  for (int n = 0; n < samples; ++n) {
    const int segment = n / 2400;
    float note = -1.5f + segment * 0.7f;
    if (segment == 3) note = -1.0f + 3.0f * float(n % 2400) / 2400.0f;
    if (segment == 5) note = 4.5f;
    const float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    float in[4] = {
        f0,
        2.5f + 2.6f * sinf(float(n) * 0.0013f),         // centroid
        2.5f + 2.6f * sinf(float(n) * 0.0021f + 1.0f),  // slope
        segment < 2 ? 0.0f : 2.5f + 2.6f * sinf(float(n) * 0.0009f + 2.0f),  // bumps
    };

    for (int k = 0; k < 3; ++k) sum[k] += in[1 + k];
    ++count;
    if (n % kBlock == 0) {
      const float centroid = knob(sum[0] / float(count));
      const float morph = knob(sum[1] / float(count));
      const float raw_bumps = knob(sum[2] / float(count));
      sum[0] = sum[1] = sum[2] = 0.0f;
      count = 0;
      const float raw_slope = (1.0f - 0.6f * raw_bumps) * morph;
      const float slope = 0.01f + 1.99f * raw_slope * raw_slope * raw_slope;
      const float bumps = 16.0f * raw_bumps * raw_bumps;
      engine.UpdateAmplitudes(centroid, slope, bumps, &engine.amplitudes_[0],
                              integer_indices, 24);
      engine.UpdateAmplitudes(centroid, slope, bumps, &engine.amplitudes_[24],
                              organ_indices, 8);
      for (int i = 0; i < 36; ++i) {
        increment[i] = (engine.amplitudes_[i] - value[i]) / float(kBlock);
      }
    }
    float ramped[36];
    for (int i = 0; i < 36; ++i) {
      value[i] += increment[i];
      ramped[i] = value[i];
    }

    float result[2] = {0.0f, 0.0f};
    engine.harmonic_oscillator_[0].Render<1>(f0, &ramped[0], &result[0], 1);
    engine.harmonic_oscillator_[1].Render<13>(f0, &ramped[12], &result[0], 1);
    engine.harmonic_oscillator_[2].Render<1>(f0, &ramped[24], &result[1], 1);

    fwrite(in, sizeof(float), 4, f);
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
