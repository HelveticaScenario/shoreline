// Plaits' chiptune engine (chiptune_engine.cc, compiled into this driver for
// its state) with its Render rebuilt a sample at a time from its unmodified
// parts: the chord bank and its inversions and sort,
// the arpeggiator and its pattern quantiser, the super square voices and the
// NES triangle bass, each rendering one sample, with Render's envelope applied
// per sample. The chord knob is the chord's number and the pattern knob the
// pattern's, each fed to its quantiser as the parameter that lands on it; the
// envelope's shape is 1 minus the decay knob (0–1), negative unless the bass
// decays too, as the port maps them. stmlib's random generator starts from
// the seed the test gives the port. Each record is one sample's inputs
// (Plaits' note in cycles per sample, chord, inversion, pattern, shape and
// decay in volts, whether the bass decays, and the trigger: -1 unpatched, 0
// low, 1 rising) then the chord and bass samples, before the output gain.
// build: plaits/dsp/chords/chord_bank.cc stmlib/dsp/units.cc stmlib/utils/random.cc

#include <cmath>
#include <cstdio>

#define private public
#include "plaits/dsp/engine2/chiptune_engine.cc"
#undef private
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

const double kC4 = 261.6255653005986;
const uint32_t kSeed = 0x1234567;

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
  static char memory[4096];
  stmlib::BufferAllocator allocator(memory, sizeof(memory));
  stmlib::Random::Seed(kSeed);
  ChiptuneEngine engine;
  engine.Init(&allocator);
  engine.Reset();
  for (int i = 0; i < kChordNumVoices; ++i) engine.voice_[i].Init();

  const int length = 4800;
  const int samples = 10 * length;
  for (int n = 0; n < samples; ++n) {
    const int segment = n / length;
    const int k = n % length;
    const float t = float(k) / float(length);
    float note = -1.0f, chord = 10.0f, inversion = 2.5f, pattern = 0.0f;
    float shape = 2.5f, decay = 5.0f, bass_decay = 0.0f;
    float trigger = -1.0f;
    switch (segment) {
      case 1: chord = 0.0013f + 9.97f * t; inversion = 1.3f; shape = 1.0f; break;
      case 2: chord = 4.0f; inversion = 5.0f * t; shape = 3.5f; break;
      case 3: chord = 9.0f; inversion = 3.0f; shape = 5.0f * t; break;
      case 4:
        chord = 9.0f; pattern = 0.0f;
        trigger = k % 600 == 0 ? 1.0f : 0.0f;
        break;
      case 5:
        chord = 5.0f; pattern = 7.0f; decay = 2.0f; bass_decay = 1.0f; shape = 0.7f;
        trigger = k % 400 == 0 ? 1.0f : 0.0f;
        break;
      case 6:
        chord = 6.0f; pattern = 10.0f; decay = 3.5f; note = -2.0f;
        trigger = k % 300 == 0 ? 1.0f : 0.0f;
        break;
      case 7:
        chord = 10.0f - 10.0f * t; pattern = 11.0f * t; decay = 1.0f;
        bass_decay = 1.0f; shape = 4.5f;
        trigger = k % 500 == 0 ? 1.0f : 0.0f;
        break;
      case 8:  // high pitch steps, into the voices' frequency limit
        chord = 8.0f; inversion = 4.6f; shape = 4.0f;
        note = 1.0f + float((n / 600) % 4);
        break;
      case 9:  // a fast arpeggio over four octaves, high, decaying hard
        chord = 7.0f; pattern = 2.0f; decay = 0.3f; bass_decay = 1.0f; note = 1.5f;
        trigger = k % 120 == 0 ? 1.0f : 0.0f;
        break;
    }
    const float f0 = float(kC4 * exp2(double(note)) / 48000.0);
    const float in[8] = {f0, chord, inversion, pattern, shape, decay, bass_decay,
                         trigger};
    fwrite(in, sizeof(float), 8, f);

    const float voice_shape = knob(shape) * 0.995f;
    const bool clocked = trigger >= 0.0f;
    float root_transposition = 1.0f;
    float out = 0.0f;
    if (clocked) {
      if (trigger > 0.0f) {
        engine.chords_.set_chord((chord + 0.5f) / (float(kChordNumChords) * 1.02f));
        engine.chords_.Sort();
        int p = engine.arpeggiator_pattern_selector_.Process((pattern + 0.5f) / 12.0f);
        engine.arpeggiator_.set_mode(ArpeggiatorMode(p / 3));
        engine.arpeggiator_.set_range(1 << (p % 3));
        engine.arpeggiator_.Clock(engine.chords_.num_notes());
        engine.envelope_state_ = 1.0f;
      }
      const float octave = float(1 << engine.arpeggiator_.octave());
      const float note_f0 = f0 * engine.chords_.sorted_ratio(
          engine.arpeggiator_.note()) * octave;
      root_transposition = octave;
      engine.voice_[0].Render(note_f0, voice_shape, &out, 1);
    } else {
      float ratios[kChordNumVoices];
      float amplitudes[kChordNumVoices];
      engine.chords_.set_chord((chord + 0.5f) / (float(kChordNumChords) * 1.02f));
      engine.chords_.ComputeChordInversion(knob(inversion), ratios, amplitudes);
      for (int j = 1; j < kChordNumVoices; j += 2) amplitudes[j] = -amplitudes[j];
      for (int v = 0; v < kChordNumVoices; ++v) {
        float x;
        engine.voice_[v].Render(f0 * ratios[v], voice_shape, &x, 1);
        out += x * amplitudes[v];
      }
    }
    float bass;
    engine.bass_.Render(f0 * 0.5f * root_transposition, &bass, 1);

    if (clocked) {
      const float magnitude = 1.0f - knob(decay);
      const float envelope_shape = bass_decay > 0.0f ? magnitude : -magnitude;
      const float shape = fabsf(envelope_shape);
      const float d = 1.0f - 2.0f / kSampleRate * SemitonesToRatio(60.0f * shape) * shape;
      float aux_envelope_amount = envelope_shape * 20.0f;
      CONSTRAIN(aux_envelope_amount, 0.0f, 1.0f);
      ONE_POLE(engine.aux_envelope_amount_, aux_envelope_amount, 0.01f);
      engine.envelope_state_ *= d;
      out *= engine.envelope_state_;
      bass *= 1.0f + engine.aux_envelope_amount_ * (engine.envelope_state_ - 1.0f);
    }
    const float result[2] = {out, bass};
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
