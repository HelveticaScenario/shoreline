// Plaits' two bass drums (AnalogBassDrum through Overdrive, and
// SyntheticBassDrum) with Operator's control model: each renders one sample
// at a time, so its controls are set from that sample's pitch and knobs, with
// exact pitches, knobs in volts, a per-sample trigger and the overdrive's
// drive faded by pitch as the bass drum engine fades it. A negative `dirt`
// stands for an unpatched one, which follows the engine's triggered formula.
// Each record is one sample's inputs then the overdriven analog drum and the
// synthetic drum, before the output gains.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc

#include <algorithm>
#include <cmath>
#include <cstdio>

#include "plaits/dsp/drums/analog_bass_drum.h"
#include "plaits/dsp/drums/synthetic_bass_drum.h"
#include "plaits/dsp/fx/overdrive.h"
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

// Zero-initialised, as Plaits' engines are: SyntheticBassDrum::Init leaves
// its transient envelope unset.
static AnalogBassDrum analog;
static Overdrive overdrive;
static SyntheticBassDrum synthetic;

const int kSegment = 9600;
const int kSegments = 8;

// The pitch in cycles per sample for segment `s` at sample `n`.
static float Pitch(int s, int n) {
  switch (s) {
    case 4:
      // Swept by an audio-rate sine around 60 Hz, as FM sweeps it.
      return 60.0f * float(exp2(double(0.8f * sinf(float(n) * 0.013f)))) /
             48000.0f;
    case 5:
      // Up to where the drive and dirt fade out, then near 1 Hz.
      return (n % kSegment < kSegment / 2 ? 2500.0f : 1.5f) / 48000.0f;
    default:
      return (45.0f + 5.0f * float(s)) / 48000.0f;
  }
}

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  analog.Init();
  overdrive.Init();
  synthetic.Init();
  Random::Seed(0x1234567u);

  for (int n = 0; n < kSegment * kSegments; ++n) {
    const int s = n / kSegment;
    const int i = n % kSegment;
    const float t = float(n);
    // Segments 0-5 strike; 6 drones with the trigger unpatched; 7 strikes
    // again.
    const bool sustain = s == 6;
    bool trigger;
    switch (s) {
      case 3:
        // Retriggered mid-decay at uneven gaps, then at audio rate.
        trigger = i < 9000 ? (i % 1373) % 457 == 0 : i % 2 == 0;
        break;
      default:
        trigger = !sustain && i % 2400 == 0;
        break;
    }
    float in[12] = {
        Pitch(s, n),
        2.5f,  // tone
        2.5f,  // decay
        4.0f,  // accent
        5.0f,  // attack FM
        5.0f,  // self FM
        0.0f,  // drive
        -1.0f, // dirt
        5.0f,  // FM envelope
        0.0f,  // FM decay
        sustain ? 1.0f : 0.0f,
        trigger ? 1.0f : 0.0f,
    };
    switch (s) {
      case 0:
        // Accents across the range.
        in[3] = 2.5f + 2.6f * sinf(t * 0.0011f);
        break;
      case 1:
        // Tone, decay and the synthetic drum's FM envelope swept.
        in[1] = 2.5f + 2.6f * sinf(t * 0.00071f);
        in[2] = 2.5f + 2.6f * sinf(t * 0.00053f + 1.0f);
        in[8] = 2.5f + 2.6f * sinf(t * 0.00037f + 2.0f);
        in[9] = 2.5f + 2.6f * sinf(t * 0.00029f + 3.0f);
        break;
      case 2:
        // Attack FM, self FM, drive and dirt swept.
        in[4] = 2.5f + 2.6f * sinf(t * 0.00067f);
        in[5] = 2.5f + 2.6f * sinf(t * 0.00043f + 1.0f);
        in[6] = 2.5f + 2.6f * sinf(t * 0.00031f + 2.0f);
        in[7] = 2.5f + 2.4f * sinf(t * 0.00023f + 3.0f);
        break;
      case 3:
      case 4:
      case 5:
        in[2] = 4.0f;
        in[6] = 3.0f;
        break;
      case 6:
        // The drone: decay sets its level; tone, self FM and dirt move.
        in[1] = 2.5f + 2.6f * sinf(t * 0.00071f);
        in[2] = 2.5f + 2.6f * sinf(t * 0.00053f + 1.0f);
        in[5] = 2.5f + 2.6f * sinf(t * 0.00043f + 1.0f);
        in[7] = i < kSegment / 2 ? -1.0f : 2.5f + 2.4f * sinf(t * 0.0011f);
        break;
    }

    const float f0 = in[0];
    const float decay = knob(in[2]);
    float out, aux;
    analog.Render(sustain, trigger, knob(in[3]), f0, knob(in[1]), decay,
                  knob(in[4]), knob(in[5]), &out, 1);
    const float drive = knob(in[6]) * std::max(1.0f - 16.0f * f0, 0.0f);
    overdrive.Process(0.5f + 0.5f * drive, &out, 1);
    const float dirtiness =
        in[7] < 0.0f ? 0.4f - 0.25f * decay * decay : knob(in[7]);
    synthetic.Render(sustain, trigger, knob(in[3]), f0, knob(in[1]), decay,
                     dirtiness, knob(in[8]), knob(in[9]), &aux, 1);
    float result[2] = {out, aux};
    fwrite(in, sizeof(float), 12, f);
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
