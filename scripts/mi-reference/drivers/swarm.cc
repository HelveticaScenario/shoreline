// Plaits' swarm engine (swarm_engine.h) with Operator's control model: each
// voice's GrainEnvelope steps every 12 samples (and on a burst's sample) from
// the knobs' mean since the last step; each voice's pitch ratio and level ramp
// across the step; Plaits' own AdditiveSawOscillator, FastSineOscillator and
// stmlib Limiter render one sample at a time from that sample's pitch. The
// first step starts each ratio at its value. Each case is a fresh swarm from
// the same random seed. Each record is one sample's inputs (pitch, density,
// spread, grain size, burst) then the limited saw sum and the sine sum,
// before the output gain.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc

#include <cmath>
#include <cstdio>

#include "plaits/dsp/engine/swarm_engine.h"
#include "stmlib/dsp/limiter.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

struct Case {
  float note;
  float spread;      // volts; < 0 sweeps it
  bool burst_mode;
};

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  const Case cases[] = {
      {-1.0f, 0.0f, false},   // unison: the sines stay at one pitch
      {0.0f, -1.0f, false},   // spread sweeping
      {1.5f, 3.5f, false},    // wide and high
      {-2.0f, -1.0f, true},   // bursts
  };
  for (const Case& c : cases) {
    Random::Seed(0x21);
    GrainEnvelope envelope[kNumSwarmVoices];
    AdditiveSawOscillator saw[kNumSwarmVoices];
    FastSineOscillator sine[kNumSwarmVoices];
    float ratio[kNumSwarmVoices], ratio_increment[kNumSwarmVoices];
    float level[kNumSwarmVoices], level_increment[kNumSwarmVoices];
    for (int i = 0; i < kNumSwarmVoices; ++i) {
      envelope[i].Init();
      saw[i].Init();
      sine[i].Init();
      ratio[i] = ratio_increment[i] = level[i] = level_increment[i] = 0.0f;
    }
    Limiter limiter;
    limiter.Init();
    float sum[3] = {0.0f, 0.0f, 0.0f};
    float count = 0.0f;
    float countdown = 0.0f;
    bool started = false;

    for (int n = 0; n < 9600; ++n) {
      float note = c.note;
      if (c.spread != 0.0f && n >= 4800) note += 0.5f;
      const float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
      const bool start_burst = c.burst_mode && n % 3000 == 100;
      float in[5] = {
          f0,
          2.5f + 2.6f * sinf(float(n) * 0.0007f),  // density
          c.spread < 0.0f ? 2.5f + 2.6f * sinf(float(n) * 0.0011f + 1.0f)
                          : c.spread,              // spread
          2.5f + 2.6f * sinf(float(n) * 0.0005f + 2.0f),  // grain size
          start_burst ? 1.0f : 0.0f,
      };

      for (int k = 0; k < 3; ++k) sum[k] += in[1 + k];
      count += 1.0f;
      if (countdown <= 0.0f || start_burst) {
        countdown = 12.0f;
        const float timbre = knob(sum[0] / count);
        const float harmonics = knob(sum[1] / count);
        const float morph = knob(sum[2] / count);
        sum[0] = sum[1] = sum[2] = 0.0f;
        count = 0.0f;
        const float density = NoteToFrequency(timbre * 120.0f) * 0.025f * 12.0f;
        const float spread = harmonics * harmonics * harmonics;
        float size_ratio = 0.25f * SemitonesToRatio((1.0f - morph) * 84.0f);
        const bool jump = !started;
        started = true;
        for (int i = 0; i < kNumSwarmVoices; ++i) {
          const float rank = (float(i) - 3.0f) / 3.0f;
          envelope[i].Step(density, c.burst_mode, start_burst);
          const float target_level =
              envelope[i].amplitude(size_ratio) * (1.0f / kNumSwarmVoices);
          const float expo_amount = envelope[i].frequency(size_ratio);
          const float target_ratio =
              SemitonesToRatio(48.0f * expo_amount * spread * rank) *
              (1.0f + rank * (rank + 0.01f) * spread * 0.25f);
          if (jump) {
            ratio[i] = target_ratio;
            ratio_increment[i] = 0.0f;
          } else {
            ratio_increment[i] = (target_ratio - ratio[i]) / 12.0f;
          }
          level_increment[i] = (target_level - level[i]) / 12.0f;
          size_ratio *= 0.97f;
        }
      }
      countdown -= 1.0f;

      float saws = 0.0f;
      float sines = 0.0f;
      for (int i = 0; i < kNumSwarmVoices; ++i) {
        ratio[i] += ratio_increment[i];
        level[i] += level_increment[i];
        const float voice_f0 = f0 * ratio[i];
        saw[i].Render(voice_f0, level[i], &saws, 1);
        sine[i].Render(voice_f0, level[i], &sines, 1);
      }
      limiter.Process(3.0f, &saws, 1);
      float result[2] = {saws, sines};

      fwrite(in, sizeof(float), 5, f);
      fwrite(result, sizeof(float), 2, f);
    }
  }
  fclose(f);
  return 0;
}
