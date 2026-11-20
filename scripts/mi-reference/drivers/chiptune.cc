// Plaits' chiptune engine (chiptune_engine.cc, compiled into this driver for
// its state) with its Render rebuilt a sample at a time from its unmodified
// parts: the chord bank and its inversions and sort, the arpeggiator and its
// pattern quantiser, the super square voices and the NES triangle bass, each
// rendering one sample, with Render's envelope applied per sample. Unclocked,
// the chord's voices play two octaves above the note, as the port's do, so
// close position stands on it; the bass and the arpeggio keep Plaits'
// pitches. The chord knob is the chord's number and the pattern knob the
// pattern's, each fed to its quantiser as the parameter that lands on it; the
// envelope's shape is 1 minus the decay knob (0–1), negative unless the bass
// decays too, as the port maps them. stmlib's random generator starts from
// the seed the test gives the port. Before writing, arpeggios run through
// the engine's own Render with its envelope set, and the driver fails unless
// the rebuilt arpeggio matches it sample for sample. Each record is one
// sample's inputs (the note in cycles per sample, chord, inversion, pattern,
// shape and decay in volts, whether the bass decays, and the trigger: -1
// unpatched, 0 low, 1 rising) then the chord and bass samples, before the
// output gain.
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

struct Inputs {
  float f0, chord, inversion, pattern, shape, decay, bass_decay, trigger;
};

static float ChordParameter(float chord) {
  return (chord + 0.5f) / (float(kChordNumChords) * 1.02f);
}

static float PatternParameter(float pattern) { return (pattern + 0.5f) / 12.0f; }

static float EnvelopeShape(const Inputs& in) {
  const float magnitude = 1.0f - knob(in.decay);
  return in.bass_decay > 0.0f ? magnitude : -magnitude;
}

static void Start(ChiptuneEngine* engine, char* memory, size_t size) {
  stmlib::BufferAllocator allocator(memory, size);
  engine->Init(&allocator);
  engine->Reset();
  for (int i = 0; i < kChordNumVoices; ++i) engine->voice_[i].Init();
}

// One sample of Render, rebuilt.
static void Rebuilt(ChiptuneEngine& engine, const Inputs& in, float* out, float* bass) {
  const float f0 = in.f0;
  const float voice_shape = knob(in.shape) * 0.995f;
  const bool clocked = in.trigger >= 0.0f;
  float root_transposition = 1.0f;
  *out = 0.0f;
  if (clocked) {
    if (in.trigger > 0.0f) {
      engine.chords_.set_chord(ChordParameter(in.chord));
      engine.chords_.Sort();
      int p = engine.arpeggiator_pattern_selector_.Process(PatternParameter(in.pattern));
      engine.arpeggiator_.set_mode(ArpeggiatorMode(p / 3));
      engine.arpeggiator_.set_range(1 << (p % 3));
      engine.arpeggiator_.Clock(engine.chords_.num_notes());
      engine.envelope_state_ = 1.0f;
    }
    const float octave = float(1 << engine.arpeggiator_.octave());
    const float note_f0 = f0 * engine.chords_.sorted_ratio(
        engine.arpeggiator_.note()) * octave;
    root_transposition = octave;
    engine.voice_[0].Render(note_f0, voice_shape, out, 1);
  } else {
    float ratios[kChordNumVoices];
    float amplitudes[kChordNumVoices];
    engine.chords_.set_chord(ChordParameter(in.chord));
    engine.chords_.ComputeChordInversion(knob(in.inversion), ratios, amplitudes);
    for (int j = 1; j < kChordNumVoices; j += 2) amplitudes[j] = -amplitudes[j];
    const float chord_f0 = f0 * 4.0f;
    for (int v = 0; v < kChordNumVoices; ++v) {
      float x;
      engine.voice_[v].Render(chord_f0 * ratios[v], voice_shape, &x, 1);
      *out += x * amplitudes[v];
    }
  }
  engine.bass_.Render(f0 * 0.5f * root_transposition, bass, 1);

  if (clocked) {
    const float envelope_shape = EnvelopeShape(in);
    const float shape = fabsf(envelope_shape);
    const float d = 1.0f - 2.0f / kSampleRate * SemitonesToRatio(60.0f * shape) * shape;
    float aux_envelope_amount = envelope_shape * 20.0f;
    CONSTRAIN(aux_envelope_amount, 0.0f, 1.0f);
    ONE_POLE(engine.aux_envelope_amount_, aux_envelope_amount, 0.01f);
    engine.envelope_state_ *= d;
    *out *= engine.envelope_state_;
    *bass *= 1.0f + engine.aux_envelope_amount_ * (engine.envelope_state_ - 1.0f);
  }
}

// Arpeggios through the unmodified Render, a sample per call, with the
// envelope set as voice.cc sets it from TIMBRE's attenuverter, against the
// rebuilt step. Returns false on any difference.
static bool CheckAgainstRender() {
  struct Run {
    float note, chord, pattern, shape, decay, bass_decay;
    int period;
  };
  const Run runs[] = {
      {45.0f, 5.0f, 7.0f, 0.7f, 2.0f, 1.0f, 400},
      {50.0f, 6.0f, 10.0f, 2.5f, 3.5f, 0.0f, 300},
      {72.0f, 9.0f, 2.0f, 4.5f, 0.3f, 1.0f, 120},
      {40.0f, 2.0f, 4.0f, 1.0f, 5.0f, 1.0f, 500},
      {57.0f, 3.0f, 8.0f, 3.3f, 0.0f, 0.0f, 250},
  };
  const int samples = 6000;
  static char memory[2][4096];
  for (const Run& run : runs) {
    ChiptuneEngine real, rebuilt;
    Start(&real, memory[0], sizeof(memory[0]));
    Start(&rebuilt, memory[1], sizeof(memory[1]));
    Inputs in = {NoteToFrequency(run.note), run.chord, 0.0f, run.pattern,
                 run.shape, run.decay, run.bass_decay, 0.0f};
    real.set_envelope_shape(EnvelopeShape(in));
    EngineParameters p;
    p.note = run.note;
    p.harmonics = ChordParameter(run.chord);
    p.timbre = PatternParameter(run.pattern);
    p.morph = knob(run.shape);
    p.accent = 0.8f;
    static float expected[samples][2];
    stmlib::Random::Seed(kSeed);
    for (int n = 0; n < samples; ++n) {
      const bool rising = n % run.period == 0;
      p.trigger = rising ? TRIGGER_RISING_EDGE | TRIGGER_HIGH : TRIGGER_LOW;
      bool already_enveloped = false;
      real.Render(p, &expected[n][0], &expected[n][1], 1, &already_enveloped);
    }
    stmlib::Random::Seed(kSeed);
    for (int n = 0; n < samples; ++n) {
      in.trigger = n % run.period == 0 ? 1.0f : 0.0f;
      float out, bass;
      Rebuilt(rebuilt, in, &out, &bass);
      if (out != expected[n][0] || bass != expected[n][1]) {
        fprintf(stderr, "note %g sample %d: rebuilt %g %g, Render %g %g\n",
                run.note, n, out, bass, expected[n][0], expected[n][1]);
        return false;
      }
    }
  }
  return true;
}

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  if (!CheckAgainstRender()) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  static char memory[4096];
  ChiptuneEngine engine;
  Start(&engine, memory, sizeof(memory));
  stmlib::Random::Seed(kSeed);

  const int length = 4800;
  const int samples = 10 * length;
  for (int n = 0; n < samples; ++n) {
    const int segment = n / length;
    const int k = n % length;
    const float t = float(k) / float(length);
    float note = -1.0f, chord = 10.0f, inversion = 0.0f, pattern = 0.0f;
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
        note = float((n / 600) % 4) - 1.0f;
        break;
      case 9:  // a fast arpeggio over four octaves, high, decaying hard
        chord = 7.0f; pattern = 2.0f; decay = 0.3f; bass_decay = 1.0f; note = 1.5f;
        trigger = k % 120 == 0 ? 1.0f : 0.0f;
        break;
    }
    const Inputs in = {float(kC4 * exp2(double(note)) / 48000.0), chord, inversion,
                       pattern, shape, decay, bass_decay, trigger};
    fwrite(&in, sizeof(float), 8, f);
    float result[2];
    Rebuilt(engine, in, &result[0], &result[1]);
    fwrite(result, sizeof(float), 2, f);
  }
  fclose(f);
  return 0;
}
