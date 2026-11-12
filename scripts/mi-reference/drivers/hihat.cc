// Plaits' two hi-hats with Operator's control model: each renders one sample
// at a time, so its controls are set from that sample's pitch and knobs, with
// exact pitches, knobs in volts and a per-sample trigger. Both run
// unmodified: HiHat<SquareNoise, SwingVCA, true, false> (the engine's out)
// and HiHat<RingModNoise, LinearVCA, false, true> (its aux). Each draws from
// its own seed of stmlib's random generator, as each module channel has its
// own. Each record is one sample's inputs then the two hats, before the
// output gains.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc

#include <cmath>
#include <cstdio>

#include "plaits/dsp/drums/hi_hat.h"
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

static HiHat<SquareNoise, SwingVCA, true, false> square_hat;
static HiHat<RingModNoise, LinearVCA, false, true> ring_hat;

const int kSegment = 9600;
const int kSegments = 9;

// The pitch in cycles per sample for segment `s` at sample `n`.
static float Pitch(int s, int n) {
  switch (s) {
    case 4:
      // Swept by an audio-rate sine around 300 Hz, as FM sweeps it.
      return 300.0f * float(exp2(double(0.8f * sinf(float(n) * 0.013f)))) /
             48000.0f;
    case 5:
      // Past the square oscillators' and the noise clock's caps, then near
      // 1 Hz.
      return (n % kSegment < kSegment / 2 ? 13000.0f : 1.5f) / 48000.0f;
    default:
      return (200.0f + 40.0f * float(s)) / 48000.0f;
  }
}

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  square_hat.Init();
  ring_hat.Init();
  uint32_t square_seed = 0x1234567u;
  uint32_t ring_seed = 0x7654321u;

  for (int n = 0; n < kSegment * kSegments; ++n) {
    const int s = n / kSegment;
    const int i = n % kSegment;
    const float t = float(n);
    // Segments 0-5 strike; 6 hisses with the trigger unpatched; 7 strikes
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
    float in[7] = {
        Pitch(s, n),
        2.5f,  // tone
        2.5f,  // decay
        0.0f,  // noisiness
        4.0f,  // accent
        sustain ? 1.0f : 0.0f,
        trigger ? 1.0f : 0.0f,
    };
    switch (s) {
      case 0:
        // Accents across the range, the softest strikes starting below the
        // ring hat's choke.
        in[4] = 2.5f + 2.6f * sinf(t * 0.0011f);
        break;
      case 1:
        // Tone and decay swept.
        in[1] = 2.5f + 2.6f * sinf(t * 0.00071f);
        in[2] = 2.5f + 2.6f * sinf(t * 0.00053f + 1.0f);
        break;
      case 2:
        // Noisiness, tone and decay swept.
        in[3] = 2.5f + 2.6f * sinf(t * 0.00067f);
        in[1] = 2.5f + 2.6f * sinf(t * 0.00043f + 1.0f);
        in[2] = 2.5f + 2.6f * sinf(t * 0.00031f + 2.0f);
        break;
      case 3:
        in[2] = 4.0f;
        in[3] = 2.0f;
        break;
      case 4:
        in[1] = 3.5f;
        in[3] = 4.0f;
        break;
      case 5:
        in[1] = 4.5f;
        in[3] = 3.0f;
        break;
      case 6:
        // The hiss: accent and decay set its level; every knob moves.
        in[1] = 2.5f + 2.6f * sinf(t * 0.00071f);
        in[2] = 2.5f + 2.6f * sinf(t * 0.00053f + 1.0f);
        in[3] = 2.5f + 2.6f * sinf(t * 0.00043f + 2.0f);
        in[4] = 2.5f + 2.6f * sinf(t * 0.00037f + 3.0f);
        break;
      case 7:
        in[3] = 1.5f;
        break;
      case 8:
        // A decay that falls through the ring hat's choke within the segment.
        in[2] = 3.0f;
        break;
    }

    const float f0 = in[0];
    const float tone = knob(in[1]);
    const float decay = knob(in[2]);
    const float noisiness = knob(in[3]);
    const float accent = knob(in[4]);
    float temp_1[1], temp_2[1], out[1], aux[1];
    Random::Seed(square_seed);
    square_hat.Render(sustain, trigger, accent, f0, tone, decay, noisiness,
                      temp_1, temp_2, out, 1);
    square_seed = Random::state();
    Random::Seed(ring_seed);
    ring_hat.Render(sustain, trigger, accent, f0, tone, decay, noisiness,
                    temp_1, temp_2, aux, 1);
    ring_seed = Random::state();
    float result[2] = {out[0], aux[0]};
    fwrite(in, sizeof(float), 7, f);
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
