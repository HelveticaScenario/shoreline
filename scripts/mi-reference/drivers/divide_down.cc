// Plaits' divide-down oscillator (string_synth_oscillator.h) with the string
// machine's registrations (string_machine_engine.cc, compiled into this
// driver) and Operator's control model: the registration knob smoothed every
// sample with the time constant of Plaits' block smoothing, starting at its
// first value, and the oscillator rendering one sample at a time from that
// sample's pitch. Each record is one sample's inputs (pitch, registration)
// then the output sample, before the output gain.
// build: plaits/dsp/chords/chord_bank.cc stmlib/dsp/units.cc

#include <cmath>
#include <cstdio>

#define private public
#include "plaits/dsp/engine2/string_machine_engine.cc"
#undef private

using namespace plaits;

const float kC4 = 261.625565f;

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

  const int samples = 14400;
  for (int n = 0; n < samples; ++n) {
    const int segment = n / 2400;
    float note = -2.5f + segment * 0.9f;
    if (segment == 3) note = -1.0f + 5.0f * float(n % 2400) / 2400.0f;
    if (segment == 5) note = 5.4f;  // into the octave shift
    const float f0 = kC4 * powf(2.0f, note) * (1.0f / 48000.0f);
    float in[2] = {f0, 2.5f + 2.6f * sinf(float(n) * 0.0013f)};

    const float target = knob(in[1]);
    smoothed = n == 0 ? target : smoothed + smoothing * (target - smoothed);
    const float gain = std::min(std::max(4.0f - fabsf(f0) * 32.0f, 0.0f), 1.0f);
    float harmonics[kChordNumHarmonics * 2 + 2];
    engine.ComputeRegistration(smoothed, harmonics);
    harmonics[kChordNumHarmonics * 2] = 0.0f;
    float out = 0.0f;
    oscillator.Render(f0, harmonics, gain, &out, 1);

    fwrite(in, sizeof(float), 2, f);
    fwrite(&out, sizeof(float), 1, f);
  }
  fclose(f);
  return 0;
}
