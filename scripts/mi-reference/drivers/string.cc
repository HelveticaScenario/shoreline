// Plaits' inharmonic string engine (string_engine.cc + StringVoice + String)
// with Operator's control model: each StringVoice renders one sample at a
// time, so its filters and delay are set from that sample's controls, with
// exact pitches, knobs in volts and a per-sample rising-edge trigger. On a
// trigger the outgoing string keeps the pitch it had, and the output runs
// through Plaits' limiter per sample. Each record is one sample's inputs then
// its limited `out` and its `aux`, before the output gains.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc plaits/dsp/physical_modelling/string_voice.cc plaits/dsp/physical_modelling/string.cc

#include <cmath>
#include <cstdio>

#include "plaits/dsp/physical_modelling/string_voice.h"
#include "stmlib/dsp/limiter.h"
#include "stmlib/utils/buffer_allocator.h"
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;
const int kNumStrings = 3;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

static char memory[65536];

int main(int argc, char** argv) {
  BufferAllocator allocator(memory, sizeof(memory));
  StringVoice voice[kNumStrings];
  float f0s[kNumStrings];
  for (int i = 0; i < kNumStrings; ++i) {
    voice[i].Init(&allocator);
    f0s[i] = 0.01f;
  }
  int active = kNumStrings - 1;
  Limiter limiter;
  limiter.Init();
  Random::Seed(0x1234567u);

  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  const int samples = 28000;
  for (int n = 0; n < samples; ++n) {
    const int segment = n / 4000;
    // Down to ~33 Hz, below the delay line's reach, then up the keyboard.
    const float note = -3.0f + segment * 0.8f;
    const float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    // Segments 0-4 pluck on a trigger; 5-6 sustain on dust.
    const bool sustain = segment >= 5;
    const bool trigger = !sustain && (n % 700) == 0;
    float in[7] = {
        f0,
        2.5f + 2.6f * sinf(float(n) * 0.00041f),        // structure
        2.5f + 2.6f * sinf(float(n) * 0.00023f + 1.0f), // brightness
        2.5f + 2.6f * sinf(float(n) * 0.00017f + 2.0f), // damping
        4.0f + 1.0f * sinf(float(n) * 0.0007f),         // accent
        sustain ? 1.0f : 0.0f,
        trigger ? 1.0f : 0.0f,
    };
    if (trigger) {
      active = (active + 1) % kNumStrings;
    }
    f0s[active] = f0;
    const float timbre = knob(in[2]);
    float temp, out = 0.0f, aux = 0.0f;
    for (int i = 0; i < kNumStrings; ++i) {
      voice[i].Render(sustain && i == active, trigger && i == active,
                      knob(in[4]), f0s[i], knob(in[1]), timbre * timbre,
                      knob(in[3]), &temp, &out, &aux, 1);
    }
    limiter.Process(1.0f, &out, 1);
    float result[2] = {out, aux};
    fwrite(in, sizeof(float), 7, f);
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
