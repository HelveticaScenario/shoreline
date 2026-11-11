// Plaits' divide-down oscillator (string_synth_oscillator.h) with the string
// machine's registrations (string_machine_engine.cc, compiled into this
// driver) and Operator's control model: the registration knob smoothed every
// sample with the time constant of Plaits' block smoothing, starting at its
// first value and landing on its target once within 1e-6, and the oscillator
// rendering one sample at a time from that sample's pitch. When the octave
// shift moves, the oscillator's pending sample is redrawn as the new saw
// levels' waveform, as the port does. Each record is one sample's inputs (pitch,
// registration) then the output sample, before the output gain.
// build: plaits/dsp/chords/chord_bank.cc stmlib/dsp/units.cc

#include <cmath>
#include <cstdio>

#define private public
#include "plaits/dsp/engine2/string_machine_engine.cc"
#undef private

using namespace plaits;

const float kC4 = 261.625565f;

// The saw levels Render derives, for the redraw on an octave shift.
static void SawGains(const float* unshifted, int shift, float gain, float* g) {
  float r[7];
  std::fill(&r[0], &r[shift], 0.0f);
  std::copy(&unshifted[0], &unshifted[7 - shift], &r[shift]);
  g[0] = (r[0] + 2.0f * r[1]) * gain;
  g[1] = (r[2] - r[1] + 2.0f * r[3]) * gain;
  g[2] = (r[4] - r[3] + 2.0f * r[5]) * gain;
  g[3] = (r[6] - r[5]) * gain;
}

static float Naive(const StringSynthOscillator& o, const float* g) {
  const int segment = o.segment_;
  return (o.phase_ - 4.0f) * g[0] * 0.125f +
         (o.phase_ - float(segment & 4) - 2.0f) * g[1] * 0.25f +
         (o.phase_ - float(segment & 6) - 1.0f) * g[2] * 0.5f +
         (o.phase_ - float(segment & 7) - 0.5f) * g[3];
}

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  StringMachineEngine engine;
  StringSynthOscillator oscillator;
  oscillator.Init();
  const float smoothing = float(1.0 - pow(0.9, 1.0 / 12.0));
  float smoothed = 0.0f;
  int last_shift = -1;

  const int samples = 21600;
  for (int n = 0; n < samples; ++n) {
    const int segment = n / 2400;
    float note = -2.5f + segment * 0.9f;
    if (segment == 3) note = -1.0f + 5.0f * float(n % 2400) / 2400.0f;
    if (segment == 5) note = 5.4f;  // past the fade
    if (segment == 6) note = 4.33f;  // halfway into the fade, shifted an octave
    if (segment == 7) note = (n / 37) % 2 ? 3.4f : 3.65f;  // across the shift
    if (segment == 8) note = 3.0f + 1.0f * float(n % 2400) / 2400.0f;
    const float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    float in[2] = {f0, 2.5f + 2.6f * sinf(float(n) * 0.0013f)};

    const float target = knob(in[1]);
    if (n == 0) {
      smoothed = target;
    } else {
      smoothed += smoothing * (target - smoothed);
      if (fabsf(target - smoothed) < 1e-6f) smoothed = target;
    }
    const float gain = std::min(std::max(4.0f - fabsf(f0) * 32.0f, 0.0f), 1.0f);
    float harmonics[kChordNumHarmonics * 2 + 2];
    engine.ComputeRegistration(smoothed, harmonics);
    harmonics[kChordNumHarmonics * 2] = 0.0f;
    float shifted = f0 * 8.0f;
    int shift = 0;
    while (shifted > 0.5f) {
      shift += 2;
      shifted *= 0.5f;
    }
    float new_gains[4];
    SawGains(harmonics, shift, gain, new_gains);
    if (last_shift >= 0 && shift != last_shift) {
      oscillator.next_sample_ = Naive(oscillator, new_gains);
    }
    last_shift = shift;
    float out = 0.0f;
    oscillator.Render(f0, harmonics, gain, &out, 1);

    fwrite(in, sizeof(float), 2, f);
    fwrite(&out, sizeof(float), 1, f);
  }
  fclose(f);
  return 0;
}
