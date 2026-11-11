// Plaits' word-bank speech as its speech engine runs it (speech_engine.cc,
// harmonics past 1/3) with Operator's control model: LPCSpeechSynth and
// LPCSpeechSynthWordBank, unmodified, driven by a copy of
// LPCSpeechSynthController::Render that renders one sample at a time, then
// Plaits' limiter. The copy differs from Plaits in two marked places, as
// `$mi.speak` does:
//  - `word` is a word index. A trigger says the word at the nearest whole
//    index; a scan reaches the word at index i between i - 0.5 and i + 0.5,
//    turned into the controller's 0-1 address.
//  - With the trigger patched, the voice is silent (an all-zero frame) until
//    the first trigger and after a bank change, where Plaits scans.
// Rendering one sample at a time also scans every sample, where Plaits scans
// once per 12-sample block. A scan parked between a voiced and an unvoiced
// frame moves the pitch towards the voiced one on each scan, so with prosody
// up it gets there sooner than in Plaits; where it settles is the same.
// A trigger reaches the controller on its sample; an unpatched trigger runs
// it free, at unity gain, and a patched one applies the accent as gain. The
// controller takes its pitch in cycles per sample of Plaits' corrected rate,
// so the voice sounds at the given frequency. Each case is a fresh controller
// from the same random seed. Each record is one sample's inputs (pitch in
// cycles per sample; bank; word index; formant shift, prosody, speed and
// accent in volts; trigger, -1 when unpatched) then the limited voice and the
// excitation, before the output gains.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc plaits/dsp/speech/lpc_speech_synth.cc plaits/dsp/speech/lpc_speech_synth_controller.cc plaits/dsp/speech/lpc_speech_synth_phonemes.cc plaits/dsp/speech/lpc_speech_synth_words.cc

#include <algorithm>
#include <cmath>
#include <cstdio>

#include "plaits/dsp/oscillator/oscillator.h"
#include "plaits/dsp/speech/lpc_speech_synth_controller.h"
#include "plaits/dsp/speech/lpc_speech_synth_words.h"
#include "stmlib/dsp/limiter.h"
#include "stmlib/dsp/parameter_interpolator.h"
#include "stmlib/dsp/units.h"
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace stmlib;

const float kC4 = 261.625565f;
const int kSamples = 24000;
const int kWords[LPC_SPEECH_SYNTH_NUM_WORD_BANKS] = {7, 11, 26, 26, 22};
const LPCSpeechSynth::Frame kSilence[2] = {};

// LPCSpeechSynthController::Render on a word bank, one sample per call.
class Controller {
 public:
  void Init(LPCSpeechSynthWordBank* word_bank) {
    word_bank_ = word_bank;
    clock_phase_ = 0.0f;
    playback_frame_ = -1;
    last_playback_frame_ = -1;
    remaining_frame_samples_ = 0;
    std::fill(&sample_[0], &sample_[2], 0.0f);
    std::fill(&next_sample_[0], &next_sample_[2], 0.0f);
    gain_ = 0.0f;
    waiting_ = true;
    synth_.Init();
  }

  void Render(bool free_running, bool trigger, int bank, float frequency,
              float prosody_amount, float speed, float address,
              float formant_shift, float gain, float* excitation,
              float* output) {
    const size_t size = 1;
    const float rate_ratio = SemitonesToRatio((formant_shift - 0.5f) * 36.0f);
    const float rate = rate_ratio / 6.0f;
    const float pitch_shift =
        frequency /
        (rate_ratio * kLPCSpeechSynthDefaultF0 / kCorrectedSampleRate);
    const float time_stretch = SemitonesToRatio(
        -speed * 24.0f +
        (formant_shift < 0.4f
             ? (formant_shift - 0.4f) * -45.0f
             : (formant_shift > 0.6f ? (formant_shift - 0.6f) * -45.0f
                                     : 0.0f)));

    if (word_bank_->Load(bank)) {
      playback_frame_ = -1;
      last_playback_frame_ = -1;
      waiting_ = true;  // Operator: silent after a bank change.
    }
    if (free_running) waiting_ = true;
    const int num_frames = word_bank_->num_frames();
    const LPCSpeechSynth::Frame* frames = word_bank_->frames();

    if (trigger) {
      waiting_ = false;
      word_bank_->GetWordBoundaries(address, &playback_frame_,
                                    &last_playback_frame_);
      remaining_frame_samples_ = 0;
    }

    if (playback_frame_ == -1 && remaining_frame_samples_ == 0) {
      if (waiting_ && !free_running) {
        // Operator: a patched trigger is silent until it says a word.
        synth_.PlayFrame(kSilence, 0.0f, true);
      } else {
        synth_.PlayFrame(frames,
                         address * (static_cast<float>(num_frames) - 1.0001f),
                         true);
      }
    } else {
      if (remaining_frame_samples_ == 0) {
        synth_.PlayFrame(frames, float(playback_frame_), false);
        remaining_frame_samples_ =
            kSampleRate / kLPCSpeechSynthFPS * time_stretch;
        ++playback_frame_;
        if (playback_frame_ >= last_playback_frame_) {
          playback_frame_ = free_running ? -1 : last_playback_frame_;
        }
      }
      remaining_frame_samples_ -= std::min(size, remaining_frame_samples_);
    }

    ParameterInterpolator gain_modulation(&gain_, gain, size);
    float this_sample[2];
    std::copy(&next_sample_[0], &next_sample_[2], &this_sample[0]);
    std::fill(&next_sample_[0], &next_sample_[2], 0.0f);
    clock_phase_ += rate;
    if (clock_phase_ >= 1.0f) {
      clock_phase_ -= 1.0f;
      float reset_time = clock_phase_ / rate;
      float new_sample[2];
      synth_.Render(prosody_amount, pitch_shift, &new_sample[0],
                    &new_sample[1], 1);
      float discontinuity[2] = {new_sample[0] - sample_[0],
                                new_sample[1] - sample_[1]};
      this_sample[0] += discontinuity[0] * ThisBlepSample(reset_time);
      next_sample_[0] += discontinuity[0] * NextBlepSample(reset_time);
      this_sample[1] += discontinuity[1] * ThisBlepSample(reset_time);
      next_sample_[1] += discontinuity[1] * NextBlepSample(reset_time);
      std::copy(&new_sample[0], &new_sample[2], &sample_[0]);
    }
    next_sample_[0] += sample_[0];
    next_sample_[1] += sample_[1];
    const float g = gain_modulation.Next();
    *excitation = this_sample[0] * g;
    *output = this_sample[1] * g;
  }

 private:
  float clock_phase_;
  float sample_[2];
  float next_sample_[2];
  float gain_;
  bool waiting_;
  LPCSpeechSynth synth_;
  int playback_frame_;
  int last_playback_frame_;
  size_t remaining_frame_samples_;
  LPCSpeechSynthWordBank* word_bank_;
};

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

static float clamp(float x, float lo, float hi) {
  return x < lo ? lo : (x > hi ? hi : x);
}

// A knob held at `volts`, or for a negative value, swept past both ends of
// `centre` ± `centre` at a rate set by its magnitude.
static float Knob(float volts, int n, float centre) {
  return volts >= 0.0f
             ? volts
             : centre + (centre + 0.1f) * sinf(float(n) * 0.0001f * -volts);
}

// The controller's address for word index `word` of `bank`, loaded into
// `lookup`: the nearest whole index on a trigger, else the scan position.
static float Address(LPCSpeechSynthWordBank* lookup, int bank, float word,
                     bool trigger) {
  const int words = kWords[bank];
  const float last = float(words - 1);
  if (trigger) {
    return (clamp(roundf(word), 0.0f, last) + 0.5f) / float(words);
  }
  word = clamp(word, -0.5f, last + 0.5f);
  const float index = clamp(roundf(word), 0.0f, last);
  int start, end;
  lookup->GetWordBoundaries((index + 0.5f) / float(words), &start, &end);
  const float position =
      float(start) + (word - index + 0.5f) * float(end + 1 - start);
  return clamp(position / (float(lookup->num_frames()) - 1.0001f), 0.0f, 1.0f);
}

struct Case {
  int bank;            // -1 steps through the banks every 5000 samples
  float voct;          // pitch around which a slow vibrato moves
  int trigger_period;  // samples between triggers; 0 for unpatched
  int unpatch_after;   // the sample the trigger is unpatched at; 0 for never
  int repatch_after;   // the sample it is patched again at; 0 for never
  float word;          // word index; < 0 sweeps
  float shift, prosody, speed, accent;  // volts; < 0 sweeps
};

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  const Case cases[] = {
      {0, -1.25f, 0, 0, 0, -3.0f, 2.5f, 0.0f, 2.5f, 4.0f},  // colours, scanned
      {1, -1.0f, 0, 0, 0, -5.0f, -7.0f, 5.0f, 2.5f, 4.0f},  // scan, prosody
      {2, -1.5f, 6000, 0, 0, -2.0f, 2.5f, 0.0f, 5.0f, 4.0f},  // fast letters
      {3, -1.0f, 9000, 0, 0, -4.0f, 2.5f, 5.0f, -6.0f, -9.0f},  // speed, accent
      {4, -1.25f, 8000, 0, 0, -3.0f, -5.0f, 2.5f, 3.5f, 4.0f},  // shift sweep
      {-1, -1.0f, 7000, 0, 0, -3.0f, 2.5f, 5.0f, 4.0f, 4.0f},  // bank switches
      {1, 3.75f, 4000, 0, 0, -4.0f, 2.5f, 5.0f, 4.0f, 5.0f},   // high pitch
      {4, -1.0f, 30000, 0, 0, 13.0f, 2.5f, 2.5f, 2.5f, 3.0f},  // one word
      {3, -1.25f, 2000, 6100, 0, -2.0f, 2.5f, 5.0f, 4.5f, 4.0f},  // unpatched
      {4, -0.5f, 0, 0, 0, -1.5f, -3.0f, -8.0f, 2.5f, 4.0f},  // prosody sweep
      {2, -1.0f, 3000, 7000, 13000, -2.5f, 2.5f, 2.5f, 4.0f, 4.0f},  // repatch
  };
  const float lpc_pitch_scale = float(48000.0 / double(kCorrectedSampleRate));
  static char memory[65536];
  static char lookup_memory[65536];
  for (const Case& c : cases) {
    Random::Seed(0x21);
    BufferAllocator allocator(memory, sizeof(memory));
    LPCSpeechSynthWordBank word_bank;
    word_bank.Init(word_banks_, LPC_SPEECH_SYNTH_NUM_WORD_BANKS, &allocator);
    BufferAllocator lookup_allocator(lookup_memory, sizeof(lookup_memory));
    LPCSpeechSynthWordBank lookup;
    lookup.Init(word_banks_, LPC_SPEECH_SYNTH_NUM_WORD_BANKS,
                &lookup_allocator);
    Controller controller;
    controller.Init(&word_bank);
    Limiter limiter;
    limiter.Init();

    for (int n = 0; n < kSamples; ++n) {
      const double voct = double(c.voct) + 0.3 * sin(double(n) * 0.0005);
      const float f0 = float(double(kC4) * exp2(voct) / 48000.0);
      const int bank = c.bank >= 0 ? c.bank : (n / 5000) % 5;
      lookup.Load(bank);
      const bool unpatched =
          c.unpatch_after && n >= c.unpatch_after &&
          (!c.repatch_after || n < c.repatch_after);
      const bool patched = c.trigger_period && !unpatched;
      const bool trigger =
          patched && n % c.trigger_period == c.trigger_period / 2;
      float in[8] = {
          f0,
          float(bank),
          Knob(c.word, n, 12.5f),
          Knob(c.shift, n, 2.5f),
          Knob(c.prosody, n, 2.5f),
          Knob(c.speed, n, 2.5f),
          Knob(c.accent, n, 2.5f),
          patched ? (trigger ? 1.0f : 0.0f) : -1.0f,
      };

      float aux, out;
      controller.Render(!patched, trigger, bank, f0 * lpc_pitch_scale,
                        knob(in[4]), knob(in[5]) * 2.0f - 1.0f,
                        Address(&lookup, bank, in[2], trigger), knob(in[3]),
                        patched ? knob(in[6]) : 1.0f, &aux, &out);
      limiter.Process(0.7f, &out, 1);
      float result[2] = {out, aux};

      fwrite(in, sizeof(float), 8, f);
      fwrite(result, sizeof(float), 2, f);
    }
  }
  fclose(f);
  return 0;
}
