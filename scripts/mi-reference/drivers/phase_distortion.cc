// Plaits' phase distortion engine (engine2/phase_distortion_engine.cc) with
// Operator's control model: controls set every sample, exact pitch
// conversions, knobs in volts, Plaits' own VariableShapeOscillator rendering
// the two oversampled substeps of each sample. Each record is one sample's
// inputs then its synced (`out`) and free-running (`aux`) samples, before the
// output gain.

#include <algorithm>
#include <cmath>
#include <cstdio>

#include "plaits/dsp/oscillator/sine_oscillator.h"
#include "plaits/dsp/oscillator/variable_shape_oscillator.h"
#include "plaits/resources.h"
#include "stmlib/dsp/dsp.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

int main(int argc, char** argv) {
  VariableShapeOscillator shaper, modulator;
  shaper.Init();
  modulator.Init();

  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  const int samples = 9600;
  for (int n = 0; n < samples; ++n) {
    const float t = float(n) / samples;
    const int segment = n / 1600;
    float note = -2.0f + segment * 0.9f;
    if (segment == 2) note = -1.0f + 4.0f * t;
    const float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    float in[4] = {
        f0,
        2.5f + 2.6f * sinf(float(n) * 0.0011f),        // ratio
        2.5f + 2.6f * sinf(float(n) * 0.0023f + 1.0f), // amount
        2.5f + 2.6f * sinf(float(n) * 0.0017f + 2.0f), // asymmetry
    };
    const float harmonics = knob(in[1]);
    const float timbre = knob(in[2]);
    const float morph = knob(in[3]);

    const float carrier = 0.5f * f0;
    const float semitones = Interpolate(lut_fm_frequency_quantizer, harmonics, 128.0f);
    const float modulator_f = std::min(
        0.25f, carrier * float(exp2(double(semitones / 12.0f))));
    const float pw = 0.5f + morph * 0.49f;
    const float amount = 8.0f * timbre * timbre * (1.0f - modulator_f * 3.8f);

    float synced[2], free_running[2];
    shaper.Render<true, true>(carrier, modulator_f, pw, 0.0f, amount, synced, 2);
    modulator.Render<false, true>(carrier, modulator_f, pw, 0.0f, amount, free_running, 2);
    float out[2] = {
        0.5f * Sine(synced[0] + 0.25f),
        0.5f * Sine(free_running[0] + 0.25f),
    };
    out[0] += 0.5f * Sine(synced[1] + 0.25f);
    out[1] += 0.5f * Sine(free_running[1] + 0.25f);

    fwrite(in, sizeof(float), 4, f);
    fwrite(out, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
