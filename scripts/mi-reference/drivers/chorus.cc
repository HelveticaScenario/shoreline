// Plaits' ensemble (fx/ensemble.h), unmodified, processing one stereo sample
// at a time with that sample's amount and depth. Each record is one sample's
// inputs (left, right, amount, depth, both 0–1) then the left and right
// outputs.

#include <cmath>
#include <cstdio>

#include "plaits/dsp/fx/ensemble.h"

using namespace plaits;

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  static float memory[1024];
  Ensemble ensemble;
  ensemble.Init(memory);
  ensemble.Reset();

  const int samples = 48000;
  for (int n = 0; n < samples; ++n) {
    const float t = float(n) / 48000.0f;
    float in[4] = {
        0.6f * sinf(6.2831853f * 220.0f * t) + 0.3f * sinf(6.2831853f * 1375.0f * t),
        0.6f * sinf(6.2831853f * 330.0f * t + 1.0f) - 0.2f * sinf(6.2831853f * 2911.0f * t),
        0.5f + 0.5f * sinf(float(n) * 0.0003f),         // amount
        0.5f + 0.5f * sinf(float(n) * 0.00021f + 1.0f), // depth
    };
    float left = in[0], right = in[1];
    ensemble.set_amount(in[2]);
    ensemble.set_depth(in[3]);
    ensemble.Process(&left, &right, 1);
    float out[2] = {left, right};
    fwrite(in, sizeof(float), 4, f);
    fwrite(out, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
