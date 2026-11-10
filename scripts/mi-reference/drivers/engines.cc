// Each ported Plaits engine, unmodified, through its own Render in Plaits'
// 12-sample blocks with the trigger unpatched, followed only by the output
// stage the `$mi.*` module includes: the engine's out and aux gains from
// voice.cc, and Plaits' limiter where that gain is negative. This pins the
// knob mappings and output levels, which the per-sample fixtures leave to
// the ports. Each case is a header (engine, MIDI note, harmonics, timbre,
// morph, sample count) then that many samples of `out` and `aux` in volts
// (±5 V full scale, the sign Plaits' output stage gives). String and modal,
// excited by random grains, run long enough for their levels to average out.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc plaits/dsp/engine/additive_engine.cc plaits/dsp/engine/fm_engine.cc plaits/dsp/engine/grain_engine.cc plaits/dsp/engine/modal_engine.cc plaits/dsp/engine/string_engine.cc plaits/dsp/engine/waveshaping_engine.cc plaits/dsp/engine2/phase_distortion_engine.cc plaits/dsp/physical_modelling/modal_voice.cc plaits/dsp/physical_modelling/resonator.cc plaits/dsp/physical_modelling/string_voice.cc plaits/dsp/physical_modelling/string.cc

#include <cstdio>

#include "stmlib/dsp/limiter.h"
#include "plaits/dsp/engine/additive_engine.h"
#include "plaits/dsp/engine/fm_engine.h"
#include "plaits/dsp/engine/grain_engine.h"
#include "plaits/dsp/engine/modal_engine.h"
#include "plaits/dsp/engine/string_engine.h"
#include "plaits/dsp/engine/waveshaping_engine.h"
#include "plaits/dsp/engine2/phase_distortion_engine.h"

using namespace plaits;

struct Case {
  int engine;
  float note, harmonics, timbre, morph;
};

// Two settings per ported engine, numbered as in Plaits' voice.cc.
static const Case kCases[] = {
    {1, 48.0f, 0.3f, 0.6f, 0.2f},  {1, 62.0f, 0.75f, 0.35f, 0.8f},
    {9, 45.0f, 0.2f, 0.7f, 0.4f},  {9, 60.0f, 0.8f, 0.3f, 0.9f},
    {10, 41.0f, 0.4f, 0.5f, 0.3f}, {10, 57.0f, 0.65f, 0.7f, 0.8f},
    {11, 43.0f, 0.3f, 0.5f, 0.4f}, {11, 59.0f, 0.7f, 0.25f, 0.85f},
    {12, 48.0f, 0.0f, 0.4f, 0.6f}, {12, 55.0f, 0.5f, 0.7f, 0.3f},
    {19, 40.0f, 0.3f, 0.6f, 0.7f}, {19, 52.0f, 0.8f, 0.4f, 0.9f},
    {20, 45.0f, 0.4f, 0.6f, 0.7f}, {20, 57.0f, 0.75f, 0.3f, 0.5f},
};

// The engine for `index`, its out and aux gains as voice.cc registers them.
static Engine* Select(int index, float* out_gain, float* aux_gain) {
  static PhaseDistortionEngine phase_distortion;
  static WaveshapingEngine waveshaping;
  static FMEngine fm;
  static GrainEngine grain;
  static AdditiveEngine additive;
  static StringEngine string;
  static ModalEngine modal;
  switch (index) {
    case 1: *out_gain = 0.7f; *aux_gain = 0.7f; return &phase_distortion;
    case 9: *out_gain = 0.7f; *aux_gain = 0.6f; return &waveshaping;
    case 10: *out_gain = 0.6f; *aux_gain = 0.6f; return &fm;
    case 11: *out_gain = 0.7f; *aux_gain = 0.6f; return &grain;
    case 12: *out_gain = 0.8f; *aux_gain = 0.8f; return &additive;
    case 19: *out_gain = -1.0f; *aux_gain = 0.8f; return &string;
    case 20: *out_gain = -1.0f; *aux_gain = 0.8f; return &modal;
  }
  return NULL;
}

// Plaits' output stage short of the int16 conversion, in volts.
static void Output(float gain, stmlib::Limiter* limiter, float* x, size_t size) {
  if (gain < 0.0f) limiter->Process(-gain, x, size);
  const float post_gain = (gain < 0.0f ? 1.0f : gain) * -5.0f;
  for (size_t i = 0; i < size; ++i) x[i] *= post_gain;
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
    float out_gain, aux_gain;
    Engine* engine = Select(c.engine, &out_gain, &aux_gain);
    engine->Init(&allocator);
    engine->LoadUserData(NULL);
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
    const int blocks = c.engine >= 19 ? 16000 : 1000;
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
