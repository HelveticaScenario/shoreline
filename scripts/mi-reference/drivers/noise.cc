// Plaits' filtered noise engine (noise_engine.cc, noise/clocked_noise.h) with
// Operator's control model: the knobs are read every sample, and Plaits' own
// ClockedNoise, Svf filters and stmlib Limiter process one sample at a time,
// the first noise clock drawing before the second. Each case is a fresh engine
// from the same random seed. Each record is one sample's inputs (pitch in
// cycles per sample, rate, mode and resonance in volts, sync, whether the
// trigger is patched) then `out` and `aux` after their limiters, before the
// output gain.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc

#include <algorithm>
#include <cmath>
#include <cstdio>

#include "plaits/dsp/engine/engine.h"
#include "plaits/dsp/noise/clocked_noise.h"
#include "stmlib/dsp/filter.h"
#include "stmlib/dsp/limiter.h"
#include "stmlib/dsp/units.h"
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

struct Case {
  float note;
  // Volts; < 0 sweeps the knob.
  float rate, mode, resonance;
  // Samples between syncs, with the trigger patched; 0 = unpatched.
  int sync_period;
};

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  const Case cases[] = {
      {0.0f, -1.0f, 2.5f, 1.0f, 0},    // the clock sweeping
      {0.75f, 5.0f, -1.0f, 2.5f, 0},   // the mode sweeping
      {-1.0f, 4.0f, 1.0f, -1.0f, 0},   // the resonance sweeping
      {0.5f, 0.5f, 3.75f, 2.0f, 1500}, // a slow clock restarted by syncs
  };
  for (const Case& c : cases) {
    Random::Seed(0x21);
    ClockedNoise noise[2];
    noise[0].Init();
    noise[1].Init();
    Svf lp_hp, bp[2];
    lp_hp.Init();
    bp[0].Init();
    bp[1].Init();
    Limiter limiter[2];
    limiter[0].Init();
    limiter[1].Init();
    const bool patched = c.sync_period > 0;

    for (int n = 0; n < 9600; ++n) {
      const float f0 = kC4 * powf(2.0f, c.note) * (1.0f / 48000.0f);
      const bool sync = patched && n % c.sync_period == 7;
      float in[6] = {
          f0,
          c.rate < 0.0f ? 2.5f + 2.6f * sinf(float(n) * 0.0011f) : c.rate,
          c.mode < 0.0f ? 2.5f + 2.6f * sinf(float(n) * 0.0013f + 1.0f) : c.mode,
          c.resonance < 0.0f ? 2.5f + 2.6f * sinf(float(n) * 0.0007f + 2.0f)
                             : c.resonance,
          sync ? 1.0f : 0.0f,
          patched ? 1.0f : 0.0f,
      };

      const float timbre = knob(in[1]);
      const float harmonics = knob(in[2]);
      const float morph = knob(in[3]);
      const float f1 = f0 * SemitonesToRatio(harmonics * 48.0f - 24.0f);
      const float lowest = patched ? -24.0f : 0.0f;
      const float clock = NoteToFrequency(timbre * (128.0f - lowest) + lowest);
      const float q = 0.5f * SemitonesToRatio(morph * 120.0f);
      float n0, n1;
      noise[0].Render(sync, clock, &n0, 1);
      noise[1].Render(sync, clock * f1 / f0, &n1, 1);

      const float gain = 1.0f / Sqrt((0.5f + q) * 40.0f * f0);
      lp_hp.set_f_q<FREQUENCY_ACCURATE>(f0, q);
      bp[0].set_f_q<FREQUENCY_ACCURATE>(f0, q);
      bp[1].set_f_q<FREQUENCY_ACCURATE>(std::min(f1, 0.49f), q);
      float input_1 = n0 * gain;
      float input_2 = n1 * gain;
      float out;
      lp_hp.ProcessMultimodeLPtoHP(&input_1, &out, 1, harmonics);
      float aux = bp[0].Process<FILTER_MODE_BAND_PASS>(input_1) +
                  bp[1].Process<FILTER_MODE_BAND_PASS>(input_2);
      limiter[0].Process(1.0f, &out, 1);
      limiter[1].Process(1.0f, &aux, 1);
      float result[2] = {out, aux};

      fwrite(in, sizeof(float), 6, f);
      fwrite(result, sizeof(float), 2, f);
    }
  }
  fclose(f);
  return 0;
}
