// Plaits' virtual analog engine (engine/virtual_analog_engine.cc, the
// VA_VARIANT 2 it builds) with Operator's control model: controls set every
// sample, exact pitch conversions, knobs in volts, Plaits' own
// VariableShapeOscillator and VariableSawOscillator rendering one sample at a
// time. Each record is one sample's inputs then its `out` and `aux` samples,
// before the output gain.

#include <algorithm>
#include <cmath>
#include <cstdio>

#include "plaits/dsp/oscillator/variable_saw_oscillator.h"
#include "plaits/dsp/oscillator/variable_shape_oscillator.h"
#include "stmlib/dsp/dsp.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

static float ratio(float semitones) {
  return float(exp2(double(semitones / 12.0f)));
}

static const float intervals[5] = {0.0f, 7.01f, 12.01f, 19.01f, 24.01f};

static float Squash(float x) { return x * x * (3.0f - 2.0f * x); }

static float ComputeDetuning(float detune) {
  detune = 2.05f * detune - 1.025f;
  CONSTRAIN(detune, -1.0f, 1.0f);
  float sign = detune < 0.0f ? -1.0f : 1.0f;
  detune = detune * sign * 3.9999f;
  MAKE_INTEGRAL_FRACTIONAL(detune);
  float a = intervals[detune_integral];
  float b = intervals[detune_integral + 1];
  return (a + (b - a) * Squash(Squash(detune_fractional))) * sign;
}

int main(int argc, char** argv) {
  VariableShapeOscillator primary, auxiliary, sync;
  VariableSawOscillator variable_saw;
  primary.Init();
  auxiliary.Init();
  auxiliary.set_master_phase(0.25f);
  sync.Init();
  variable_saw.Init();

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
        2.5f + 2.6f * sinf(float(n) * 0.0011f),         // detune
        2.5f + 2.6f * sinf(float(n) * 0.0023f + 1.0f),  // square
        2.5f + 2.6f * sinf(float(n) * 0.0017f + 2.0f),  // shape
    };
    const float harmonics = knob(in[1]);
    const float timbre = knob(in[2]);
    const float morph = knob(in[3]);

    const float detune_ratio = ratio(ComputeDetuning(harmonics));
    const float sync_ratio = ratio(timbre * timbre * 48.0f);
    const float square_semitones =
        timbre < 0.5f ? 0.0f : (timbre - 0.5f) * (timbre - 0.5f) * 4.0f * 48.0f;
    const float square_sync_ratio = ratio(square_semitones);

    const float primary_f = f0;
    const float auxiliary_f = primary_f * detune_ratio;

    float shape = morph * 1.5f;
    CONSTRAIN(shape, 0.0f, 1.0f);
    float pw = 0.5f + (morph - 0.66f) * 1.46f;
    CONSTRAIN(pw, 0.5f, 0.995f);

    float p, a;
    primary.Render(primary_f, primary_f * sync_ratio, pw, shape, &p, 1);
    auxiliary.Render(auxiliary_f, auxiliary_f * sync_ratio, pw, shape, &a, 1);
    const float aux = (a - p) * 0.5f;

    float square_pw = 1.3f * timbre - 0.15f;
    CONSTRAIN(square_pw, 0.005f, 0.5f);
    const float square_gain = std::min(timbre * 8.0f, 1.0f);
    float saw_pw = morph < 0.5f ? morph + 0.5f : 1.0f - (morph - 0.5f) * 2.0f;
    saw_pw *= 1.1f;
    CONSTRAIN(saw_pw, 0.005f, 1.0f);
    float saw_shape = 10.0f - 21.0f * morph;
    CONSTRAIN(saw_shape, 0.0f, 1.0f);
    float saw_gain = 8.0f * (1.0f - morph);
    CONSTRAIN(saw_gain, 0.02f, 1.0f);
    const float norm = 1.0f / std::max(square_gain, saw_gain);

    float square, saw;
    sync.Render(primary_f, primary_f * square_sync_ratio, square_pw, 1.0f,
                &square, 1);
    variable_saw.Render(auxiliary_f, saw_pw, saw_shape, &saw, 1);
    const float out =
        saw * (saw_gain * 0.5f * norm) + square_gain * 0.3f * norm * square;

    const float record[2] = {out, aux};
    fwrite(in, sizeof(float), 4, f);
    fwrite(record, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
