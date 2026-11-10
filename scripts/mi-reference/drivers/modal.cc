// Plaits' modal engine (modal_engine.cc + ModalVoice + Resonator) with
// Operator's control model: ModalVoice renders one sample at a time, so its
// excitation filter and modes are recomputed from that sample's controls,
// with exact pitches, knobs in volts, a per-sample rising-edge trigger, and
// the structure one-pole and output limiter run per sample. Each record is
// one sample's inputs then its limited `out` and its `aux`, before the
// output gains.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc plaits/dsp/physical_modelling/modal_voice.cc plaits/dsp/physical_modelling/resonator.cc

#include <cmath>
#include <cstdio>

#include "plaits/dsp/physical_modelling/modal_voice.h"
#include "stmlib/dsp/dsp.h"
#include "stmlib/dsp/limiter.h"
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

int main(int argc, char** argv) {
  ModalVoice voice;
  voice.Init();
  Limiter limiter;
  limiter.Init();
  Random::Seed(0x1234567u);
  // Plaits' 0.01-per-12-sample-block structure filter as a per-sample one-pole.
  const float structure_coefficient = float(1.0 - pow(0.99, 1.0 / 12.0));
  float structure_lp = 0.0f;

  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  const int samples = 24000;
  for (int n = 0; n < samples; ++n) {
    const int segment = n / 4000;
    const float note = -2.0f + segment * 0.7f;
    const float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    // Segments 0-3 strike on a trigger; 4-5 sustain on dust.
    const bool sustain = segment >= 4;
    const bool trigger = !sustain && (n % 1000) == 0;
    float in[7] = {
        f0,
        2.5f + 2.6f * sinf(float(n) * 0.00031f),        // structure
        2.5f + 2.6f * sinf(float(n) * 0.00023f + 1.0f), // brightness
        2.5f + 2.6f * sinf(float(n) * 0.00017f + 2.0f), // damping
        4.0f + 1.0f * sinf(float(n) * 0.0007f),         // accent
        sustain ? 1.0f : 0.0f,
        trigger ? 1.0f : 0.0f,
    };
    ONE_POLE(structure_lp, knob(in[1]), structure_coefficient);
    float temp, out = 0.0f, aux = 0.0f;
    voice.Render(sustain, trigger, knob(in[4]), f0, structure_lp, knob(in[2]),
                 knob(in[3]), &temp, &out, &aux, 1);
    limiter.Process(1.0f, &out, 1);
    float result[2] = {out, aux};
    fwrite(in, sizeof(float), 7, f);
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
