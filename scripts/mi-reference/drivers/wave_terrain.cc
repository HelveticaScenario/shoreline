// Plaits' wave terrain engine (wave_terrain_engine.cc, compiled into this
// driver) with Operator's control model: controls set every sample, knobs in
// volts, and the engine's own path oscillator and Terrain functions rendering
// one sample (two substeps) at a time. Each case is a fresh engine at one
// pitch, its path started at that pitch, with the knobs sweeping every
// terrain: the port holds the path's radius as the pitch moves, where
// Plaits' path drifts. Each record is one
// sample's inputs then its `out` and `aux` samples, before the output gain.
// build: stmlib/dsp/units.cc

#include <cmath>
#include <cstdio>

#define private public
#include "plaits/dsp/engine2/wave_terrain_engine.cc"
#undef private

using namespace plaits;

const float kC4 = 261.625565f;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  const float notes[] = {-3.0f, -2.0f, -1.0f, 0.0f, 1.0f, 2.0f, 3.5f, 5.5f};
  for (float note : notes) {
    static char memory[4096];
    stmlib::BufferAllocator allocator(memory, sizeof(memory));
    WaveTerrainEngine engine;
    engine.Init(&allocator);
    const float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    // The port's path starts at its first frequency rather than ramping up
    // from 0 Hz.
    engine.path_.epsilon_ = FastSineOscillator::Fast2Sin(f0 * 0.5f);
    for (int n = 0; n < 2400; ++n) {
      float in[4] = {
          f0,
          2.5f + 2.6f * sinf(float(n) * 0.003f),          // terrain
          2.5f + 2.6f * sinf(float(n) * 0.0023f + 1.0f),  // radius
          2.5f + 2.6f * sinf(float(n) * 0.0017f + 2.0f),  // offset
      };
      const float harmonics = knob(in[1]);
      const float timbre = knob(in[2]);
      const float morph = knob(in[3]);

      const float attenuation = std::max(1.0f - 8.0f * f0, 0.0f);
      const float radius =
          0.1f + 0.9f * timbre * attenuation * (2.0f - attenuation);
      float path_x[2], path_y[2];
      engine.path_.RenderQuadrature(f0 * 0.5f, radius, path_x, path_y, 2);

      const float x_offset = 1.9f * morph - 1.0f;
      float z = std::min(harmonics * 1.05f, 1.0f) * (8.0f - 1.0001f);
      MAKE_INTEGRAL_FRACTIONAL(z);
      float out_s = 0.0f;
      float aux_s = 0.0f;
      for (int j = 0; j < 2; ++j) {
        const float x = path_x[j] * (1.0f - fabsf(x_offset)) + x_offset;
        const float y = path_y[j];
        const float z0 = engine.Terrain(x, y, z_integral);
        const float z1 = engine.Terrain(x, y, z_integral + 1);
        const float height = z0 + (z1 - z0) * z_fractional;
        out_s += height;
        aux_s += y + height;
      }
      float result[2] = {0.5f * out_s, Sine(1.0f + 0.5f * 0.5f * aux_s)};

      fwrite(in, sizeof(float), 4, f);
      fwrite(result, sizeof(float), 2, f);
    }
  }
  fclose(f);
  return 0;
}
