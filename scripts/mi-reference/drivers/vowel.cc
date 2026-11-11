// Plaits' speech engine on its vowel path (speech_engine.cc) with Operator's
// control model: Plaits' own NaiveSpeechSynth, SAMSpeechSynth and
// LPCSpeechSynthController (on its vowel bank) render one sample at a time,
// blended by the model knob as SpeechEngine blends them, then Plaits' limiter.
// A trigger reaches all three synths on its sample. The LPC synth takes its
// pitch in cycles per sample of Plaits' corrected rate, so the voice sounds at
// the given frequency. Each case is a fresh engine from the same random seed.
// Each record is one sample's inputs (pitch in cycles per sample, model,
// vowel, formant shift in volts, trigger) then the limited voice and the
// excitation, before the output gains.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc plaits/dsp/speech/naive_speech_synth.cc plaits/dsp/speech/sam_speech_synth.cc plaits/dsp/speech/lpc_speech_synth.cc plaits/dsp/speech/lpc_speech_synth_controller.cc plaits/dsp/speech/lpc_speech_synth_phonemes.cc

#include <cmath>
#include <cstdio>

#include "plaits/dsp/speech/lpc_speech_synth_controller.h"
#include "plaits/dsp/speech/naive_speech_synth.h"
#include "plaits/dsp/speech/sam_speech_synth.h"
#include "stmlib/dsp/limiter.h"
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;
const int kSamples = 12000;

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

struct Case {
  float model;   // volts; < 0 sweeps 0 to 5
  float voct;    // pitch around which a slow vibrato moves
  int trigger_period;  // samples between triggers; 0 for none
  float shift;   // volts; < 0 sweeps it
};

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  const Case cases[] = {
      {-1.0f, -1.25f, 0, -1.0f},     // every model and blend
      {0.0f, -1.5f, 2400, -1.0f},    // formant filters, clicks
      {2.5f, -0.5f, 1500, 2.0f},     // SAM, consonants
      {5.0f, -1.0f, 1700, -1.0f},    // LPC, consonants across the shift
      {1.25f, 0.0f, 1300, 3.5f},     // formant filters into SAM
      {3.75f, -2.0f, 1900, 1.0f},    // SAM into LPC
      {-1.0f, 3.75f, 2100, 0.0f},    // pitch past SAM's and LPC's limits
  };
  const float lpc_pitch_scale = float(48000.0 / double(kCorrectedSampleRate));
  for (const Case& c : cases) {
    Random::Seed(0x21);
    NaiveSpeechSynth naive;
    naive.Init();
    SAMSpeechSynth sam;
    sam.Init();
    LPCSpeechSynthController lpc;
    lpc.Init(NULL);
    Limiter limiter;
    limiter.Init();

    for (int n = 0; n < kSamples; ++n) {
      const double voct = double(c.voct) + 0.3 * sin(double(n) * 0.0005);
      const float f0 = float(double(kC4) * exp2(voct) / 48000.0);
      const bool trigger = c.trigger_period && n % c.trigger_period == 11;
      float in[5] = {
          f0,
          c.model < 0.0f ? 5.0f * float(n) / float(kSamples - 1) : c.model,
          2.5f + 2.6f * sinf(float(n) * 0.0007f),
          c.shift < 0.0f ? 2.5f + 2.6f * sinf(float(n) * 0.0011f + 1.0f)
                         : c.shift,
          trigger ? 1.0f : 0.0f,
      };
      const float group = 2.0f * knob(in[1]);
      const float vowel = knob(in[2]);
      const float shift = knob(in[3]);

      float temp[2], aux, out;
      float blend = group;
      if (group <= 1.0f) {
        naive.Render(trigger, f0, vowel, shift, &temp[0], &aux, &out, 1);
      } else {
        lpc.Render(false, trigger, -1, f0 * lpc_pitch_scale, 0.0f, 0.0f,
                   vowel, shift, 1.0f, &aux, &out, 1);
        blend = 2.0f - blend;
      }
      sam.Render(trigger, f0, vowel, shift, &temp[0], &temp[1], 1);
      blend *= blend * (3.0f - 2.0f * blend);
      blend *= blend * (3.0f - 2.0f * blend);
      aux += (temp[0] - aux) * blend;
      out += (temp[1] - out) * blend;
      limiter.Process(0.7f, &out, 1);
      float result[2] = {out, aux};

      fwrite(in, sizeof(float), 5, f);
      fwrite(result, sizeof(float), 2, f);
    }
  }
  fclose(f);
  return 0;
}
