// Plaits' waveshaping engine (waveshaping_engine.cc) with Operator's control
// model: controls set every sample, knobs in volts, Plaits' own Oscillator on
// its through-zero path rendering one sample at a time. Each record is one
// sample's inputs then its `out` and `aux` samples, before the output gain.

#include <cmath>
#include <cstdint>
#include <cstdio>

#include "plaits/dsp/oscillator/oscillator.h"
#include "plaits/dsp/oscillator/sine_oscillator.h"
#include "plaits/resources.h"
#include "stmlib/dsp/dsp.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

static float Tame(float f0, float harmonics, float order) {
  f0 *= harmonics;
  float max_f = 0.5f / order;
  float max_amount = 1.0f - (f0 - max_f) / (0.5f - max_f);
  CONSTRAIN(max_amount, 0.0f, 1.0f);
  return max_amount * max_amount * max_amount;
}

int main(int argc, char** argv) {
  Oscillator slope, triangle;
  slope.Init();
  triangle.Init();

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
    float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    // Through-zero: the pitch sweeps down through zero and back.
    if (segment == 4) f0 = 0.01f * sinf(float(n) * 0.002f);
    float in[4] = {
        f0,
        2.5f + 2.6f * sinf(float(n) * 0.0011f),        // shape
        2.5f + 2.6f * sinf(float(n) * 0.0023f + 1.0f), // fold
        2.5f + 2.6f * sinf(float(n) * 0.0017f + 2.0f), // asymmetry
    };
    const float harmonics = knob(in[1]);
    const float timbre = knob(in[2]);
    const float morph = knob(in[3]);

    const float pw = morph * 0.45f + 0.5f;
    const float zero = 0.0f;
    float out, aux;
    slope.Render<OSCILLATOR_SHAPE_SLOPE, true, true>(f0, pw, &zero, &out, 1);
    triangle.Render<OSCILLATOR_SHAPE_SLOPE, true, true>(f0, 0.5f, &zero, &aux, 1);

    const float pitch = fabsf(f0);
    const float slope_amount = 3.0f + fabsf(morph - 0.5f) * 5.0f;
    const float shape_amount = fabsf(harmonics - 0.5f) * 2.0f;
    const float shape_attenuation = Tame(pitch, slope_amount, 16.0f);
    const float wavefolder_attenuation = Tame(
        pitch,
        slope_amount * (3.0f + shape_amount * shape_attenuation * 5.0f),
        12.0f);
    const float shape_target = 0.5f + (harmonics - 0.5f) * shape_attenuation;
    const float wavefolder_gain = 0.03f + 0.46f * timbre * wavefolder_attenuation;
    const float overtone_gain_1 = timbre * (2.0f - timbre);
    const float overtone_gain = overtone_gain_1 * (2.0f - overtone_gain_1);

    float shape = shape_target * 3.9999f;
    MAKE_INTEGRAL_FRACTIONAL(shape);
    const int16_t* shape_1 = lookup_table_i16_table[shape_integral];
    const int16_t* shape_2 = lookup_table_i16_table[shape_integral + 1];
    float ws_index = 127.0f * out + 128.0f;
    MAKE_INTEGRAL_FRACTIONAL(ws_index)
    ws_index_integral &= 255;
    float x0 = static_cast<float>(shape_1[ws_index_integral]) / 32768.0f;
    float x1 = static_cast<float>(shape_1[ws_index_integral + 1]) / 32768.0f;
    float x = x0 + (x1 - x0) * ws_index_fractional;
    float y0 = static_cast<float>(shape_2[ws_index_integral]) / 32768.0f;
    float y1 = static_cast<float>(shape_2[ws_index_integral + 1]) / 32768.0f;
    float y = y0 + (y1 - y0) * ws_index_fractional;
    float mix = x + (y - x) * shape_fractional;
    float index = mix * wavefolder_gain + 0.5f;
    float fold = InterpolateHermite(lut_fold + 1, index, 512.0f);
    float fold_2 = -InterpolateHermite(lut_fold_2 + 1, index, 512.0f);
    float sine = Sine(aux * 0.25f + 0.5f);
    float result[2] = {fold, sine + (fold_2 - sine) * overtone_gain};

    fwrite(in, sizeof(float), 4, f);
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
