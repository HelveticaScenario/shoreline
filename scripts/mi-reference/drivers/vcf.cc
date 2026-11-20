// Plaits' virtual analog VCF engine (engine2/virtual_analog_vcf_engine.cc)
// with Operator's control model: controls set every sample, exact pitch
// conversions, knobs in volts, Plaits' own VariableShapeOscillator and
// stmlib's Svf. Each record is one sample's inputs then its low-pass (`out`)
// and high-pass (`aux`) samples, before the output gain.

#include <algorithm>
#include <cmath>
#include <cstdio>

#include "plaits/dsp/oscillator/variable_shape_oscillator.h"
#include "stmlib/dsp/dsp.h"
#include "stmlib/dsp/filter.h"
#include "stmlib/dsp/parameter_interpolator.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

int main(int argc, char** argv) {
  VariableShapeOscillator oscillator, sub_oscillator;
  oscillator.Init();
  sub_oscillator.Init();
  Svf svf[2];
  svf[0].Init();
  svf[1].Init();
  float previous_sub_gain = 0.0f, previous_cutoff = 0.0f;
  float previous_stage2_gain = 0.0f, previous_q = 0.0f, previous_gain = 0.0f;

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
    float note = -2.5f + segment * 0.9f;
    if (segment == 2) note = -1.0f + 4.0f * t;
    const float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    float in[5] = {
        f0,
        2.5f + 2.6f * sinf(float(n) * 0.0011f),         // cutoff
        2.5f + 2.6f * sinf(float(n) * 0.0023f + 1.0f),  // resonance
        2.5f + 2.6f * sinf(float(n) * 0.0029f + 3.0f),  // slope
        2.5f + 2.6f * sinf(float(n) * 0.0017f + 2.0f),  // shape
    };
    const float timbre = knob(in[1]);
    const float resonance = knob(in[2]);
    const float slope = knob(in[3]);
    const float morph = knob(in[4]);
    // Plaits' `harmonics` that gives this resonance on the steep and the
    // gentle side, crossfaded by slope, sets the drive.
    const float steep = 0.375f - resonance / 2.667f;
    const float gentle = 0.625f + resonance / 2.667f;
    const float harmonics = steep + (gentle - steep) * slope;

    float shape = (morph - 0.25f) * 2.0f + 0.5f;
    CONSTRAIN(shape, 0.5f, 1.0f);
    float pw = (morph - 0.5f) * 2.0f + 0.5f;
    if (morph > 0.75f) {
      pw = 2.5f - morph * 2.0f;
    }
    CONSTRAIN(pw, 0.5f, 0.98f);
    const float sub_gain = std::max(fabsf(morph - 0.5f) - 0.3f, 0.0f) * 5.0f;

    float out, aux;
    oscillator.Render(f0, pw, shape, &out, size_t(1));
    sub_oscillator.Render(f0 * 0.501f, 0.5f, 1.0f, &aux, size_t(1));

    const float cutoff_target =
        f0 * float(exp2(double((timbre - 0.2f) * 120.0f) / 12.0));
    const float stage2_gain_target = 1.0f - slope;
    const float resonance_sqr = resonance * resonance;
    const float q_target = resonance_sqr * resonance_sqr * 48.0f;
    float gain_target = (harmonics - 0.7f) + 0.85f;
    CONSTRAIN(gain_target, 0.7f - resonance_sqr * 0.3f, 1.0f);

    ParameterInterpolator sub_gain_modulation(&previous_sub_gain, sub_gain, size_t(1));
    ParameterInterpolator cutoff_modulation(&previous_cutoff, cutoff_target, size_t(1));
    ParameterInterpolator stage2_gain_modulation(
        &previous_stage2_gain, stage2_gain_target, size_t(1));
    ParameterInterpolator q_modulation(&previous_q, q_target, size_t(1));
    ParameterInterpolator gain_modulation(&previous_gain, gain_target, size_t(1));

    const float cutoff = std::min(cutoff_modulation.Next(), 0.25f);
    const float q = q_modulation.Next();
    const float stage2_gain = stage2_gain_modulation.Next();
    svf[0].set_f_q<FREQUENCY_FAST>(cutoff, 0.5f + q);
    svf[1].set_f_q<FREQUENCY_FAST>(cutoff, 0.5f + 0.025f * q);
    const float gain = gain_modulation.Next();
    const float input = SoftClip((out + aux * sub_gain_modulation.Next()) * gain);
    float lp, hp;
    svf[0].Process<FILTER_MODE_LOW_PASS, FILTER_MODE_HIGH_PASS>(input, &lp, &hp);
    lp = SoftClip(lp * gain);
    lp += stage2_gain * (SoftClip(svf[1].Process<FILTER_MODE_LOW_PASS>(lp)) - lp);
    const float result[2] = {lp, SoftClip(hp * gain)};

    fwrite(in, sizeof(float), 5, f);
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
