// Plaits' chord engine (chord_engine.cc, compiled into this driver) rebuilt a
// sample at a time from its unmodified parts: the chord bank and its
// inversions, the engine's registrations, divide-down oscillators and
// wavetable oscillators, each rendering one sample. The inversion and
// waveform knobs follow Operator's control model: smoothed every sample with
// the time constant of Plaits' block smoothing, starting at their first
// values and landing on their targets once within 1e-6. The chord knob is
// the chord's number, fed to the bank's quantiser as the parameter that
// lands on it. When a divide-down voice's octave shift moves, its pending
// sample is redrawn as the new saw levels' waveform, as the port does. The
// port scales each wavetable difference by the pitch that made it, where
// Plaits scales the filtered differences by the current pitch; when a
// wavetable voice's pitch moves, its filters' state is rescaled by the new
// pitch over the old, which makes the two the same. Each
// record is one sample's inputs (pitch in cycles per sample, chord,
// inversion and waveform in volts) then the chord and root samples, before
// the output gain.
// build: plaits/dsp/chords/chord_bank.cc stmlib/dsp/units.cc

#include <cmath>
#include <cstdio>

#define private public
#include "plaits/dsp/engine/chord_engine.cc"
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
  static char memory[4096];
  stmlib::BufferAllocator allocator(memory, sizeof(memory));
  ChordEngine engine;
  engine.Init(&allocator);
  engine.Reset();
  StringSynthOscillator divide_down[kChordNumVoices];
  WavetableOscillator<128, 15> wavetable_voice[kChordNumVoices];
  int last_shift[kChordNumVoices];
  float last_frequency[kChordNumVoices];
  for (int v = 0; v < kChordNumVoices; ++v) {
    divide_down[v].Init();
    wavetable_voice[v].Init();
    last_shift[v] = -1;
    last_frequency[v] = 0.0f;
  }
  const float smoothing = float(1.0 - pow(0.9, 1.0 / 12.0));
  float inversion_lp = 0.0f;
  float waveform_lp = 0.0f;

  const int length = 2400;
  const int samples = 10 * length;
  for (int n = 0; n < samples; ++n) {
    const int segment = n / length;
    const float t = float(n % length) / float(length);
    float note = -1.0f, chord = 10.0f, inversion = 0.0f, waveform = 0.0f;
    switch (segment) {
      case 1: chord = 0.0013f + 9.97f * t; inversion = 1.3f; waveform = 1.0f; break;
      case 2: chord = 9.9987f - 9.97f * t; inversion = 3.1f; waveform = 0.4f; break;
      case 3: chord = 4.0f; inversion = 5.0f * t; waveform = 1.5f; break;
      case 4: chord = 9.0f; inversion = 2.0f; waveform = 5.0f * t; break;
      case 5:  // pitch steps on the wavetable
        chord = 6.0f; inversion = 1.0f; waveform = 4.0f;
        note = -2.0f + float((n / 400) % 5) * 0.7f;
        break;
      case 6:  // high enough to fade and shift octaves
        chord = 8.0f; inversion = 4.6f; waveform = 1.0f; note = 2.3f;
        break;
      case 7:  // a sweep halfway into the wavetable
        chord = 3.0f; inversion = 2.7f; waveform = 2.6f; note = -2.0f + 5.0f * t;
        break;
      case 8:  // waveform jumps the smoothing glides across
        chord = 7.0f; inversion = 0.6f; waveform = (n / 300) % 2 ? 5.0f : 0.0f;
        break;
      case 9:
        chord = 5.0f; waveform = 3.5f;
        inversion = 2.5f + 2.4f * sinf(float(n) * 0.003f);
        break;
    }
    const float f0 = float(kC4 * exp2(double(note)) / 48000.0);
    const float in[4] = {f0, chord, inversion, waveform};

    if (n == 0) {
      inversion_lp = knob(inversion);
      waveform_lp = knob(waveform);
    } else {
      inversion_lp += smoothing * (knob(inversion) - inversion_lp);
      if (fabsf(knob(inversion) - inversion_lp) < 1e-6f) inversion_lp = knob(inversion);
      waveform_lp += smoothing * (knob(waveform) - waveform_lp);
      if (fabsf(knob(waveform) - waveform_lp) < 1e-6f) waveform_lp = knob(waveform);
    }
    engine.chords_.set_chord((chord + 0.5f) / (float(kChordNumChords) * 1.02f));

    float harmonics[kChordNumHarmonics * 2 + 2];
    engine.ComputeRegistration(std::max(1.0f - waveform_lp * 2.15f, 0.0f), harmonics);
    harmonics[kChordNumHarmonics * 2] = 0.0f;
    float ratios[kChordNumVoices];
    float amplitudes[kChordNumVoices];
    const int aux_mask = engine.chords_.ComputeChordInversion(
        inversion_lp, ratios, amplitudes);

    float out = 0.0f, aux = 0.0f;
    const float note_base = f0 * 0.998f;
    const float scan = std::max((waveform_lp - 0.535f) * 2.15f, 0.0f);
    for (int v = 0; v < kChordNumVoices; ++v) {
      const float wavetable_amount =
          Constrain(50.0f * (waveform_lp - fade_point[v]), 0.0f, 1.0f);
      float divide_down_amount = 1.0f - wavetable_amount;
      float* destination = (1 << v) & aux_mask ? &aux : &out;
      const float note_f0 = note_base * ratios[v];
      const float divide_down_gain = Constrain(4.0f - note_f0 * 32.0f, 0.0f, 1.0f);
      divide_down_amount *= divide_down_gain;
      if (wavetable_amount) {
        const float frequency = Constrain(note_f0 * 1.004f, 0.0000001f, kMaxFrequency);
        if (last_frequency[v] != 0.0f && frequency != last_frequency[v]) {
          const float ratio = frequency / last_frequency[v];
          wavetable_voice[v].lp_ *= ratio;
          wavetable_voice[v].differentiator_.lp_ *= ratio;
        }
        last_frequency[v] = frequency;
        wavetable_voice[v].Render(note_f0 * 1.004f,
                                  amplitudes[v] * wavetable_amount, scan,
                                  wavetable, destination, 1);
      }
      if (divide_down_amount) {
        const float gain = amplitudes[v] * divide_down_amount;
        float shifted = note_f0 * 8.0f;
        int shift = 0;
        while (shifted > 0.5f) {
          shift += 2;
          shifted *= 0.5f;
        }
        if (last_shift[v] >= 0 && shift != last_shift[v]) {
          float g[4];
          SawGains(harmonics, shift, gain, g);
          divide_down[v].next_sample_ = Naive(divide_down[v], g);
        }
        last_shift[v] = shift;
        divide_down[v].Render(note_f0, harmonics, gain, destination, 1);
      }
    }
    out += aux;
    aux *= 3.0f;

    fwrite(in, sizeof(float), 4, f);
    fwrite(&out, sizeof(float), 1, f);
    fwrite(&aux, sizeof(float), 1, f);
  }
  fclose(f);
  return 0;
}
