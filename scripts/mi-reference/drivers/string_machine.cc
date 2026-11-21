// Plaits' string machine engine (string_machine_engine.cc, compiled into this
// driver) rebuilt a sample at a time from its unmodified parts: the chord
// bank, the engine's registrations, divide-down oscillators, naive state-
// variable filters and ensemble, each rendering one sample. The registration
// knob follows Operator's control model: smoothed every sample with the time
// constant of Plaits' block smoothing, starting at its first value and
// landing on its target once within 1e-6. The chord knob is the chord's
// number, fed to the bank's quantiser as the parameter that lands on it. When
// a divide-down voice's octave shift moves, its pending sample is redrawn as
// the new saw levels' waveform, as the port does. Each record is one sample's
// inputs (Plaits' note in cycles per sample, chord, registration and tone in
// volts) then the left and right samples, before the output gain.
// build: plaits/dsp/chords/chord_bank.cc stmlib/dsp/units.cc

#include <cmath>
#include <cstdio>

#define private public
#include "plaits/dsp/engine2/string_machine_engine.cc"
#undef private

using namespace plaits;

const double kC4 = 261.6255653005986;

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

static float Constrain(float x, float lo, float hi) {
  return x < lo ? lo : (x > hi ? hi : x);
}

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  static char memory[16384];
  stmlib::BufferAllocator allocator(memory, sizeof(memory));
  StringMachineEngine engine;
  engine.Init(&allocator);
  engine.Reset();
  int last_shift[kChordNumNotes];
  for (int v = 0; v < kChordNumNotes; ++v) last_shift[v] = -1;
  const float smoothing = float(1.0 - pow(0.9, 1.0 / 12.0));
  float registration_lp = 0.0f;

  const int length = 2400;
  const int samples = 10 * length;
  for (int n = 0; n < samples; ++n) {
    const int segment = n / length;
    const float t = float(n % length) / float(length);
    float note = -1.0f, chord = 10.0f, registration = 0.0f, tone = 3.5f;
    switch (segment) {
      case 1: chord = 0.0013f + 9.97f * t; registration = 1.0f; tone = 4.0f; break;
      case 2: chord = 9.9987f - 9.97f * t; registration = 3.0f; tone = 1.0f; break;
      case 3: chord = 4.0f; registration = 5.0f * t; tone = 4.5f; break;
      case 4: chord = 9.0f; registration = 2.0f; tone = 5.0f * t; break;
      case 5:  // pitch steps
        chord = 6.0f; registration = 0.5f; tone = 3.0f;
        note = -2.0f + float((n / 400) % 5) * 0.7f;
        break;
      case 6:  // high enough to fade and shift octaves
        chord = 8.0f; registration = 4.0f; tone = 2.0f; note = 3.3f;
        break;
      case 7:  // a pitch sweep
        chord = 3.0f; registration = 2.7f; tone = 4.8f; note = -2.0f + 5.0f * t;
        break;
      case 8:  // registration jumps the smoothing glides across
        chord = 7.0f; tone = 0.3f; registration = (n / 300) % 2 ? 5.0f : 0.0f;
        break;
      case 9:
        chord = 5.0f; registration = 1.5f;
        tone = 2.5f + 2.4f * sinf(float(n) * 0.003f);
        break;
    }
    const float f0 = float(kC4 * exp2(double(note)) / 48000.0);
    const float in[4] = {f0, chord, registration, tone};

    if (n == 0) {
      registration_lp = knob(registration);
    } else {
      registration_lp += smoothing * (knob(registration) - registration_lp);
      if (fabsf(knob(registration) - registration_lp) < 1e-6f) {
        registration_lp = knob(registration);
      }
    }
    engine.chords_.set_chord((chord + 0.5f) / (float(kChordNumChords) * 1.02f));

    float harmonics[kChordNumHarmonics * 2 + 2];
    engine.ComputeRegistration(registration_lp, harmonics);
    harmonics[kChordNumHarmonics * 2] = 0.0f;

    float out = 0.0f, aux = 0.0f;
    const float note_base = f0 * 0.998f;
    for (int v = 0; v < kChordNumNotes; ++v) {
      const float note_f0 = note_base * engine.chords_.ratio(v);
      const float divide_down_gain = Constrain(4.0f - note_f0 * 32.0f, 0.0f, 1.0f);
      const float gain = 0.25f * divide_down_gain;
      float shifted = note_f0 * 8.0f;
      int shift = 0;
      while (shifted > 0.5f) {
        shift += 2;
        shifted *= 0.5f;
      }
      StringSynthOscillator& o = engine.divide_down_voice_[v];
      if (last_shift[v] >= 0 && shift != last_shift[v]) {
        float g[4];
        SawGains(harmonics, shift, gain, g);
        o.next_sample_ = Naive(o, g);
      }
      last_shift[v] = shift;
      o.Render(note_f0, harmonics, gain, v & 1 ? &aux : &out, 1);
    }

    const float timbre = knob(tone);
    const float cutoff = 2.2f * note_base * SemitonesToRatio(120.0f * timbre);
    engine.svf_[0].set_f_q<stmlib::FREQUENCY_DIRTY>(cutoff, 1.0f);
    engine.svf_[1].set_f_q<stmlib::FREQUENCY_DIRTY>(cutoff * 1.5f, 1.0f);
    const float l = engine.svf_[0].Process<stmlib::FILTER_MODE_LOW_PASS>(out);
    const float r = engine.svf_[1].Process<stmlib::FILTER_MODE_LOW_PASS>(aux);
    float left = 0.66f * l + 0.33f * r;
    float right = 0.66f * r + 0.33f * l;
    engine.ensemble_.set_amount(fabsf(timbre - 0.5f) * 2.0f);
    engine.ensemble_.set_depth(0.35f + 0.65f * timbre);
    engine.ensemble_.Process(&left, &right, 1);

    fwrite(in, sizeof(float), 4, f);
    fwrite(&left, sizeof(float), 1, f);
    fwrite(&right, sizeof(float), 1, f);
  }
  fclose(f);
  return 0;
}
