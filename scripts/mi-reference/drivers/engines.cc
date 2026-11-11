// Each ported Plaits engine, unmodified, through its own Render in Plaits'
// 12-sample blocks with the trigger unpatched, followed only by the output
// stage the `$mi.*` module includes: the engine's out and aux gains from
// voice.cc, Plaits' limiter where that gain is negative, and the clip at
// full scale. This pins the knob mappings and output levels, which the
// per-sample fixtures leave to the ports. Each case is a header (engine,
// MIDI note, harmonics, timbre, morph, sample count) then that many samples
// of `out` and `aux` in volts (±5 V full scale, the sign Plaits' output stage
// gives). Each case starts stmlib's random generator from the seed the
// modules derive for channel 0 when a test pins `TEST_SEED_BASE` to 1, so the
// random engines draw the same values as the modules; string's cases keep
// dispersion off, where only the sounding string draws.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc plaits/dsp/engine/additive_engine.cc plaits/dsp/engine/fm_engine.cc plaits/dsp/engine/grain_engine.cc plaits/dsp/engine/modal_engine.cc plaits/dsp/engine/particle_engine.cc plaits/dsp/engine/speech_engine.cc plaits/dsp/engine/string_engine.cc plaits/dsp/engine/swarm_engine.cc plaits/dsp/engine/waveshaping_engine.cc plaits/dsp/engine2/phase_distortion_engine.cc plaits/dsp/engine2/six_op_engine.cc plaits/dsp/engine2/wave_terrain_engine.cc plaits/dsp/fm/algorithms.cc plaits/dsp/fm/dx_units.cc plaits/dsp/physical_modelling/modal_voice.cc plaits/dsp/physical_modelling/resonator.cc plaits/dsp/physical_modelling/string_voice.cc plaits/dsp/physical_modelling/string.cc plaits/dsp/speech/lpc_speech_synth.cc plaits/dsp/speech/lpc_speech_synth_controller.cc plaits/dsp/speech/lpc_speech_synth_phonemes.cc plaits/dsp/speech/lpc_speech_synth_words.cc plaits/dsp/speech/naive_speech_synth.cc plaits/dsp/speech/sam_speech_synth.cc

#include <algorithm>
#include <cstdio>

#include "stmlib/dsp/limiter.h"
#include "stmlib/utils/random.h"
#include "plaits/dsp/engine/additive_engine.h"
#include "plaits/dsp/engine/fm_engine.h"
#include "plaits/dsp/engine/grain_engine.h"
#include "plaits/dsp/engine/modal_engine.h"
#include "plaits/dsp/engine/particle_engine.h"
#include "plaits/dsp/engine/speech_engine.h"
#include "plaits/dsp/engine/string_engine.h"
#include "plaits/dsp/engine/swarm_engine.h"
#include "plaits/dsp/engine/waveshaping_engine.h"
#include "plaits/dsp/engine2/phase_distortion_engine.h"
#include "plaits/dsp/engine2/six_op_engine.h"
#include "plaits/dsp/engine2/wave_terrain_engine.h"
#include "plaits/resources.h"

using namespace plaits;

struct Case {
  int engine;
  float note, harmonics, timbre, morph;
};

// The six-op engines' harmonics knob centred on patch `index`.
static float Patch(int index) { return (float(index) + 0.5f) / 32.64f; }

// Two settings per ported engine, numbered as in Plaits' voice.cc; one or two
// patches of each six-op bank; four of the speech engine's vowel path where
// harmonics sounds one synth alone: the formant filters (0), SAM (1/6) and
// LPC (just below 1/3); and two of its word banks, the colours (0.48) and
// the synth words (0.9), scanned.
static const Case kCases[] = {
    {1, 48.0f, 0.3f, 0.6f, 0.2f},  {1, 62.0f, 0.75f, 0.35f, 0.8f},
    {2, 40.0f, Patch(0), 0.6f, 0.3f}, {2, 55.0f, Patch(26), 0.45f, 0.6f},
    {3, 60.0f, Patch(0), 0.5f, 0.5f}, {3, 72.0f, Patch(20), 0.7f, 0.2f},
    {4, 55.0f, Patch(11), 0.4f, 0.7f}, {4, 48.0f, Patch(29), 0.5f, 0.4f},
    {5, 43.0f, 0.2f, 0.6f, 0.4f},  {5, 58.0f, 0.85f, 0.8f, 0.7f},
    {5, 86.0f, 0.5f, 0.9f, 0.5f},
    {9, 45.0f, 0.2f, 0.7f, 0.4f},  {9, 60.0f, 0.8f, 0.3f, 0.9f},
    {10, 41.0f, 0.4f, 0.5f, 0.3f}, {10, 57.0f, 0.65f, 0.7f, 0.8f},
    {11, 43.0f, 0.3f, 0.5f, 0.4f}, {11, 59.0f, 0.7f, 0.25f, 0.85f},
    {12, 48.0f, 0.0f, 0.4f, 0.6f}, {12, 55.0f, 0.5f, 0.7f, 0.3f},
    {15, 45.0f, 0.0f, 0.5f, 0.3f}, {15, 52.0f, 1.0f / 6.0f, 0.3f, 0.8f},
    {15, 48.0f, 1.0f / 6.0f, 0.6f, 0.2f}, {15, 40.0f, 0.333f, 0.45f, 0.1f},
    {15, 48.0f, 0.48f, 0.5f, 0.6f}, {15, 43.0f, 0.9f, 0.35f, 0.5f},
    {16, 48.0f, 0.5f, 0.5f, 0.5f}, {16, 60.0f, 0.2f, 0.7f, 0.9f},
    {18, 52.0f, 0.4f, 0.8f, 0.8f}, {18, 64.0f, 0.7f, 0.6f, 0.2f},
    {19, 40.0f, 0.1f, 0.6f, 0.7f}, {19, 52.0f, 0.25f, 0.4f, 0.9f},
    {20, 45.0f, 0.4f, 0.6f, 0.7f}, {20, 57.0f, 0.75f, 0.3f, 0.5f},
};

const uint32_t kModuleSeed = 0x910a2deb;

// The engine for `index`, its out and aux gains as voice.cc registers them.
static Engine* Select(int index, float* out_gain, float* aux_gain) {
  static PhaseDistortionEngine phase_distortion;
  static WaveTerrainEngine wave_terrain;
  static WaveshapingEngine waveshaping;
  static FMEngine fm;
  static GrainEngine grain;
  static AdditiveEngine additive;
  static StringEngine string;
  static SwarmEngine swarm;
  static ModalEngine modal;
  static ParticleEngine particle;
  static SpeechEngine speech;
  static SixOpEngine six_op;
  switch (index) {
    case 1: *out_gain = 0.7f; *aux_gain = 0.7f; return &phase_distortion;
    case 2:
    case 3:
    case 4: *out_gain = 1.0f; *aux_gain = 1.0f; return &six_op;
    case 5: *out_gain = 0.7f; *aux_gain = 0.7f; return &wave_terrain;
    case 9: *out_gain = 0.7f; *aux_gain = 0.6f; return &waveshaping;
    case 10: *out_gain = 0.6f; *aux_gain = 0.6f; return &fm;
    case 11: *out_gain = 0.7f; *aux_gain = 0.6f; return &grain;
    case 12: *out_gain = 0.8f; *aux_gain = 0.8f; return &additive;
    case 15: *out_gain = -0.7f; *aux_gain = 0.8f; return &speech;
    case 16: *out_gain = -3.0f; *aux_gain = 1.0f; return &swarm;
    case 18: *out_gain = -2.0f; *aux_gain = 1.0f; return &particle;
    case 19: *out_gain = -1.0f; *aux_gain = 0.8f; return &string;
    case 20: *out_gain = -1.0f; *aux_gain = 0.8f; return &modal;
  }
  return NULL;
}

// Plaits' output stage in volts: the int16 conversion's clip at full scale,
// without its quantisation.
static void Output(float gain, stmlib::Limiter* limiter, float* x, size_t size) {
  if (gain < 0.0f) limiter->Process(-gain, x, size);
  const float post_gain = (gain < 0.0f ? 1.0f : gain) * -5.0f;
  for (size_t i = 0; i < size; ++i) {
    x[i] = std::min(std::max(x[i] * post_gain, -5.0f), 5.0f);
  }
}

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  static char memory[65536];
  for (const Case& c : kCases) {
    stmlib::BufferAllocator allocator(memory, sizeof(memory));
    stmlib::Random::Seed(kModuleSeed);
    float out_gain, aux_gain;
    Engine* engine = Select(c.engine, &out_gain, &aux_gain);
    engine->Init(&allocator);
    engine->LoadUserData(c.engine >= 2 && c.engine <= 4
                             ? fm_patches_table[c.engine - 2]
                             : NULL);
    engine->Reset();
    stmlib::Limiter out_limiter, aux_limiter;
    out_limiter.Init();
    aux_limiter.Init();

    EngineParameters p;
    p.trigger = TRIGGER_UNPATCHED;
    p.note = c.note;
    p.harmonics = c.harmonics;
    p.timbre = c.timbre;
    p.morph = c.morph;
    p.accent = 0.8f;
    const int blocks = 1000;
    const float header[6] = {float(c.engine), c.note, c.harmonics, c.timbre,
                             c.morph, float(blocks * kBlockSize)};
    fwrite(header, sizeof(float), 6, f);
    for (int b = 0; b < blocks; ++b) {
      float out[kBlockSize], aux[kBlockSize];
      bool already_enveloped = false;
      engine->Render(p, out, aux, kBlockSize, &already_enveloped);
      Output(out_gain, &out_limiter, out, kBlockSize);
      Output(aux_gain, &aux_limiter, aux, kBlockSize);
      for (size_t i = 0; i < kBlockSize; ++i) {
        const float sample[2] = {out[i], aux[i]};
        fwrite(sample, sizeof(float), 2, f);
      }
    }
  }
  fclose(f);
  return 0;
}
