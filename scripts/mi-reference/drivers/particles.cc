// Plaits' particle engine (particle_engine.cc, noise/particle.h,
// fx/diffuser.h) with Operator's control model: every 12 samples (and on a
// sync's sample) each particle draws its block's impulses in Plaits' order
// from the density knob's mean since the last tick; the pitch, spread and
// resonance are read every sample; Plaits' own Svf filters, Diffuser and
// stmlib Limiter process one sample at a time. Each case is a fresh engine
// from the same random seed. Each record is one sample's inputs (pitch,
// density, spread, resonance, sync) then the limited, diffused output and the
// raw impulses, before the output gain.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc

#include <algorithm>
#include <cmath>
#include <cstdio>

#include "plaits/dsp/engine/engine.h"
#include "plaits/dsp/fx/diffuser.h"
#include "stmlib/dsp/filter.h"
#include "stmlib/dsp/limiter.h"
#include "stmlib/dsp/units.h"
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;
const int kParticles = 6;
const int kTick = 12;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

struct Particle {
  Svf filter;
  float pre_gain;
  float draws[kTick];
  int first_impulse;
  float frequency_draw;

  void Init() {
    filter.Init();
    pre_gain = 0.0f;
    first_impulse = -1;
  }

  void DrawTick(int samples, float density, bool sync) {
    float u = Random::GetFloat();
    if (sync) u = density;
    first_impulse = -1;
    for (int k = 0; k < samples; ++k) {
      draws[k] = u;
      if (u <= density && first_impulse < 0) {
        frequency_draw = Random::GetFloat();
        first_impulse = k;
      }
      u = Random::GetFloat();
    }
  }
};

struct Case {
  float note;
  float density;  // volts; < 0 sweeps it
  bool syncs;
};

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  static uint16_t memory[8192];
  const Case cases[] = {
      {0.0f, 3.5f, false},   // dense, the resonance sweeping
      {-1.0f, -1.0f, false}, // density sweeping
      {2.0f, 1.0f, true},    // sparse, fired by syncs
  };
  for (const Case& c : cases) {
    Random::Seed(0x21);
    Particle particles[kParticles];
    for (int i = 0; i < kParticles; ++i) particles[i].Init();
    Svf post_filter;
    post_filter.Init();
    Diffuser diffuser;
    diffuser.Init(memory);
    diffuser.Reset();
    Limiter limiter;
    limiter.Init();
    float sum = 0.0f, count = 0.0f, countdown = 0.0f;
    float density_48k = 0.0f;
    int position = 0;

    for (int n = 0; n < 9600; ++n) {
      const float f0 = kC4 * powf(2.0f, c.note) * (1.0f / 48000.0f);
      const bool sync = c.syncs && n % 2000 == 7;
      float in[5] = {
          f0,
          c.density < 0.0f ? 2.5f + 2.6f * sinf(float(n) * 0.0009f) : c.density,
          2.5f + 2.6f * sinf(float(n) * 0.0013f + 1.0f),  // spread
          2.5f + 2.6f * sinf(float(n) * 0.0007f + 2.0f),  // resonance
          sync ? 1.0f : 0.0f,
      };

      sum += in[1];
      count += 1.0f;
      if (countdown <= 0.0f || sync) {
        if (sync) countdown = 0.0f;
        const float timbre = knob(sum / count);
        sum = 0.0f;
        count = 0.0f;
        const float density_sqrt = NoteToFrequency(60.0f + timbre * timbre * 72.0f);
        density_48k = density_sqrt * density_sqrt * (1.0f / kParticles);
        countdown += float(kTick);
        for (int i = 0; i < kParticles; ++i) {
          particles[i].DrawTick(kTick, density_48k, sync);
        }
        position = 0;
      }
      countdown -= 1.0f;

      const float harmonics = knob(in[2]);
      const float morph = knob(in[3]);
      const float q_sqrt = SemitonesToRatio(morph >= 0.5f ? (morph - 0.5f) * 120.0f : 0.0f);
      const float q = 0.5f + q_sqrt * q_sqrt;
      const float spread = 48.0f * harmonics * harmonics;
      const float raw_diffusion_sqrt = 2.0f * fabsf(morph - 0.5f);
      const float raw_diffusion = raw_diffusion_sqrt * raw_diffusion_sqrt;
      const float diffusion = morph < 0.5f ? raw_diffusion : 0.0f;
      const float gain = 1.0f / density_48k;

      const int k = position++;
      float raw = 0.0f;
      float out = 0.0f;
      for (int i = 0; i < kParticles; ++i) {
        Particle& p = particles[i];
        const float u = p.draws[k];
        const float s = u <= density_48k ? u * gain : 0.0f;
        if (k == p.first_impulse) {
          const float detune = 2.0f * p.frequency_draw - 1.0f;
          const float fp = std::min(SemitonesToRatio(spread * detune) * f0, 0.25f);
          p.pre_gain = 0.5f / Sqrt(q * fp * Sqrt(density_48k));
          p.filter.set_f_q<FREQUENCY_DIRTY>(fp, q);
        }
        raw += s;
        out += p.filter.Process<FILTER_MODE_BAND_PASS>(p.pre_gain * s);
      }
      post_filter.set_f_q<FREQUENCY_DIRTY>(std::min(f0, 0.49f), 0.5f);
      out = post_filter.Process<FILTER_MODE_LOW_PASS>(out);
      diffuser.Process(0.8f * diffusion * diffusion, 0.5f * diffusion + 0.25f, &out, 1);
      limiter.Process(2.0f, &out, 1);
      float result[2] = {out, raw};

      fwrite(in, sizeof(float), 5, f);
      fwrite(result, sizeof(float), 2, f);
    }
  }
  fclose(f);
  return 0;
}
