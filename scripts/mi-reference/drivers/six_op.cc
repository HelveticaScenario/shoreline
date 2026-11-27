// Plaits' six-operator FM engine (six_op_engine.cc + fm/voice.h) and
// Operator's control model: a lane holds one more voice than it plays at
// once, each note-on takes the next silent one and fades out the oldest note
// past the count, a note that releases into silence fades out, SixOpEngine's logic runs for its sounding voices and their
// fm::Voice<6>'s envelopes, LFOs and level scaling every sample, the
// operators render through Plaits' own compiled render calls one sample at a
// time, and each voice passes through Plaits' output stage before the voices
// are summed. The pitch
// is exact: the note's frequency times 2^(pitch modulation), the note number
// for keyboard and rate scaling taken from that frequency. A patch change
// renders at once. Each case is a fresh engine from its own random seed. Each
// record is one sample's inputs (case, pitch, brightness, envelope, accent,
// gate as 1, 0 or -1 for unpatched, bank, patch, voice count) then the
// engine's soft-clipped output, before the output gain.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc plaits/dsp/engine2/six_op_engine.cc plaits/dsp/fm/algorithms.cc plaits/dsp/fm/dx_units.cc

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>

#define private public
#include "plaits/dsp/engine2/six_op_engine.h"
#undef private
#include "stmlib/utils/random.h"

using namespace plaits;
using namespace plaits::fm;
using namespace stmlib;

const float kC4 = 261.625565f;
const float kRate = 48000.0f;
const int kSamples = 8000;
const float kFade = 1.0f / (5.0e-3f * kRate);
const int kMaxVoices = 8;
// The most a lane's summed voices reach: the module's output range.
const float kAccentTrim = 2.0f;
const int kSlots = kMaxVoices + 1;
const float kSilentAmplitude = 1.0f / 1024.0f;
const float kAmplitudeFollow = std::min(1.0f / ((12.0f / 48000.0f) * kRate), 1.0f);

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

static float MidiNote(float f0) {
  double note = 69.0 + 12.0 * log2(double(fabsf(f0)) * double(kRate) / 440.0);
  return std::min(std::max(float(note), -119.0f), 120.0f);
}

// Whether `v` has released and settled below kSilentAmplitude in every
// carrier, given its operators' `targets`.
static bool Released(const Voice<6>& v, const Voice<6>::Parameters& p,
                     const float* targets) {
  if (p.gate || p.sustain) return false;
  for (int i = 0; i < 6; ++i) {
    if (v.algorithms_->is_modulator(v.patch_->algorithm, i)) continue;
    const auto& e = v.operator_envelope_[i];
    if (!(e.stage_ == 3 && e.phase_ >= 1.0f && targets[i] <= kSilentAmplitude &&
          v.operator_[i].amplitude <= kSilentAmplitude)) {
      return false;
    }
  }
  return true;
}

// fm::Voice<6>::Render for one sample, with an exact pitch `f0` (cycles per
// sample) and no blank sample after a patch change. Each operator's
// amplitude follows its level a twelfth of the way per sample at 48 kHz,
// which lags it as Plaits' ramp across a 24-sample block does on average.
static void RenderVoice(Voice<6>& v, const Voice<6>::Parameters& p, float f0_in,
                        float* out, float* targets) {
  v.Setup();
  const float envelope_rate = 1.0f;
  const float ad_scale = Pow2Fast<1>((0.5f - p.envelope_control) * 8.0f);
  const float r_scale = Pow2Fast<1>(-fabsf(p.envelope_control - 0.3f) * 8.0f);
  const float gate_duration = 1.5f * v.sample_rate_;
  const float envelope_sample = gate_duration * p.envelope_control;

  const float pitch_envelope = p.sustain
      ? v.pitch_envelope_.RenderAtSample(envelope_sample, gate_duration)
      : v.pitch_envelope_.Render(p.gate, envelope_rate, ad_scale, r_scale);
  const float pitch_mod = pitch_envelope + p.pitch_mod;
  const float f0 = f0_in * float(exp2(double(pitch_mod)));

  const bool note_on = p.gate && !v.gate_;
  v.gate_ = p.gate;
  if (note_on || p.sustain) {
    v.normalized_velocity_ = NormalizeVelocity(p.velocity);
    v.note_ = MidiNote(f0_in);
  }
  if (note_on && v.patch_->reset_phase) {
    for (int i = 0; i < 6; ++i) v.operator_[i].phase = 0;
  }

  float f[6];
  float a[6];
  for (int i = 0; i < 6; ++i) {
    const Patch::Operator& op = v.patch_->op[i];
    f[i] = v.ratios_[i] * (v.ratios_[i] < 0.0f ? -v.one_hz_ : f0);
    const float rate_scaling = RateScaling(v.note_, op.rate_scaling);
    float level = p.sustain
        ? v.operator_envelope_[i].RenderAtSample(envelope_sample, gate_duration)
        : v.operator_envelope_[i].Render(
              p.gate, envelope_rate * rate_scaling, ad_scale, r_scale);
    const float kb_scaling = KeyboardScaling(v.note_, op.keyboard_scaling);
    const float velocity_scaling =
        v.normalized_velocity_ * float(op.velocity_sensitivity);
    const float brightness = v.algorithms_->is_modulator(v.patch_->algorithm, i)
        ? (p.brightness - 0.5f) * 32.0f
        : 0.0f;
    level += 0.125f * std::min(kb_scaling + velocity_scaling + brightness,
                               v.level_headroom_[i]);
    const float sensitivity = AmpModSensitivity(op.amp_mod_sensitivity);
    const float log_level_mod = sensitivity * p.amp_mod - 1.0f;
    const float level_mod = 1.0f - Pow2Fast<2>(6.4f * log_level_mod);
    const float target = std::min(Pow2Fast<2>(-14.0f + level * level_mod), 4.0f);
    targets[i] = target;
    const float from = v.operator_[i].amplitude;
    a[i] = from + (target - from) * kAmplitudeFollow;
  }

  float temp[2] = {0.0f, 0.0f};
  float* buffers[4] = {out, &temp[0], &temp[1], &temp[1]};
  for (int i = 0; i < 6;) {
    const Algorithms<6>::RenderCall& call =
        v.algorithms_->render_call(v.patch_->algorithm, i);
    (*call.render_fn)(&v.operator_[i], &f[i], &a[i], v.feedback_state_,
                      v.patch_->feedback, buffers[call.input_index],
                      buffers[call.output_index], 1);
    i += call.n;
  }
}

enum Life { SILENT, PLAYING, FADING };

struct SixOp {
  Algorithms<6> algorithms;
  Patch patches[3][32];
  FMVoice voice[kSlots];
  Life life[kSlots];
  float gain[kSlots];
  uint32_t started[kSlots];
  // Each voice's note frequency, which only the active voice follows.
  float pitch_[kSlots];
  int active;
  uint32_t notes;
  bool gate;
  bool droning;

  void Init() {
    algorithms.Init();
    for (int i = 0; i < kSlots; ++i) {
      voice[i].Init(&algorithms, kRate);
      life[i] = SILENT;
      gain[i] = 0.0f;
      started[i] = 0;
      pitch_[i] = 0.0f;
    }
    for (int b = 0; b < 3; ++b) {
      for (int i = 0; i < 32; ++i) {
        patches[b][i].Unpack(fm_patches_table[b] + i * Patch::SYX_SIZE);
      }
    }
    active = kSlots - 1;
    notes = 0;
    gate = false;
    droning = false;
  }

  // The slot among the first `in_use` (other than `skip`, and only playing
  // ones if `playing`) whose note started longest ago.
  int Oldest(int in_use, int skip, bool playing) {
    int oldest = 0;
    uint32_t age = 0;
    bool found = false;
    for (int i = 0; i < in_use; ++i) {
      if (i == skip || (playing && life[i] != PLAYING)) continue;
      const uint32_t a = notes - started[i];
      if (!found || a > age) {
        oldest = i;
        age = a;
        found = true;
      }
    }
    return oldest;
  }

  void StartNote(int voices) {
    voices = std::min(std::max(voices, 1), kMaxVoices);
    const int in_use = voices + 1;
    int next = -1;
    for (int k = 1; k <= in_use && next < 0; ++k) {
      const int i = (active + k) % in_use;
      if (life[i] == SILENT) next = i;
    }
    for (int i = in_use; i < kSlots && next < 0; ++i) {
      if (life[i] == SILENT) next = i;
    }
    if (next < 0) next = Oldest(in_use, -1, false);
    ++notes;
    Voice<6>& v = voice[next].voice_;
    if (life[next] == SILENT) {
      // Back to where FMVoice::Init leaves it, keeping the patch.
      for (int i = 0; i < 6; ++i) {
        v.operator_[i].Reset();
        v.operator_envelope_[i].stage_ = 3;
        v.operator_envelope_[i].phase_ = 1.0f;
        v.operator_envelope_[i].start_ = 0.0f;
      }
      v.pitch_envelope_.stage_ = 3;
      v.pitch_envelope_.phase_ = 1.0f;
      v.pitch_envelope_.start_ = 0.0f;
      v.feedback_state_[0] = v.feedback_state_[1] = 0.0f;
    }
    v.gate_ = false;
    life[next] = PLAYING;
    gain[next] = 1.0f;
    started[next] = notes;
    active = next;
    for (int i = in_use; i < kSlots; ++i) {
      if (i != next && life[i] == PLAYING) life[i] = FADING;
    }
    for (;;) {
      int others = 0;
      for (int i = 0; i < in_use; ++i) {
        if (i != next && life[i] == PLAYING) ++others;
      }
      if (others < voices) break;
      life[Oldest(in_use, next, true)] = FADING;
    }
  }

  // SixOpEngine::Render for one sample; `gate` < 0 is unpatched.
  float Render(float pitch, float brightness, float envelope, float accent,
               int gate_in, int bank, int patch_index, int voices) {
    const Patch* patch = &patches[bank][patch_index];
    if (gate_in < 0) {
      if (!droning) {
        StartNote(voices);
        droning = true;
      }
      gate = false;
      voice[active].mutable_lfo()->Scrub(2.0f * kRate * envelope);
      voice[active].LoadPatch(patch);
    } else {
      droning = false;
      const bool high = gate_in > 0;
      if (high && !gate) {
        StartNote(voices);
        voice[active].LoadPatch(patch);
        voice[active].mutable_lfo()->Reset();
      }
      gate = high;
      voice[active].mutable_lfo()->Step(1.0f);
    }
    Voice<6>::Parameters* p = voice[active].mutable_parameters();
    p->sustain = gate_in < 0;
    p->gate = gate_in > 0;
    pitch_[active] = pitch;
    p->velocity = accent;
    p->envelope_control = envelope;
    p->brightness = brightness;
    voice[active].set_modulations(voice[active].lfo());
    for (int i = 0; i < kSlots; ++i) {
      if (i == active || life[i] == SILENT) continue;
      Voice<6>::Parameters* p = voice[i].mutable_parameters();
      p->brightness = brightness;
      p->sustain = false;
      p->gate = false;
      if (voice[i].patch() != voice[active].patch()) {
        voice[i].mutable_lfo()->Step(1.0f);
        voice[i].set_modulations(voice[i].lfo());
      } else {
        voice[i].set_modulations(voice[active].lfo());
      }
    }
    float out = 0.0f;
    for (int i = 0; i < kSlots; ++i) {
      if (life[i] == SILENT) continue;
      float sample = 0.0f;
      bool released = true;
      if (voice[i].patch_) {
        float targets[6];
        RenderVoice(voice[i].voice_, voice[i].parameters_, pitch_[i], &sample,
                    targets);
        released = Released(voice[i].voice_, voice[i].parameters_, targets);
      }
      sample = SoftClip(sample * 0.25f);
      if (life[i] == PLAYING) {
        out += sample;
        if (released) life[i] = FADING;
      } else {
        out += sample * gain[i];
        gain[i] -= kFade;
        if (gain[i] <= 0.0f) life[i] = SILENT;
      }
    }
    return std::min(std::max(out, -kAccentTrim), kAccentTrim);
  }
};

struct Case {
  int bank;
  int patch[2];      // alternating from note to note
  bool gated;
  int note_length;   // samples per note; the gate is high for 3/4 of it
  float note;        // V/Oct
  float vibrato;     // V/Oct depth
  float envelope;    // volts; < 0 sweeps it
  float brightness;  // volts; < 0 sweeps it
  int voices;
};

static const Case kCases[] = {
    // Three-operator feedback loop (algorithm 4), pitch envelope, sine LFO.
    {0, {26, 26}, true, 3200, -1.0f, 0.0f, 2.5f, -1.0f, 2},
    // Pitch envelope and delayed LFO, retriggered fast, envelope sweeping.
    {1, {13, 13}, true, 1200, 0.25f, 0.01f, -1.0f, 2.0f, 2},
    // Two-operator feedback loop (algorithm 6), fixed-frequency operators,
    // slow envelopes.
    {1, {25, 25}, true, 1600, -0.5f, 0.0f, 4.5f, 3.5f, 2},
    // Scrubbed: amplitude LFO, envelope and brightness sweeping.
    {2, {11, 11}, false, 8000, -1.5f, 0.0f, -1.0f, -1.0f, 2},
    // Scrubbed: fixed-frequency operators, pitch envelope, gliding pitch.
    {2, {4, 4}, false, 8000, 0.0f, 0.3f, 1.0f, 2.5f, 2},
    // Notes alternating between two patches, each retriggering in the
    // other's release.
    {2, {0, 3}, true, 2000, 0.5f, 0.0f, 3.0f, -1.0f, 2},
    // Snappy envelopes, retriggered in their releases.
    {0, {18, 9}, true, 2600, -2.0f, 0.02f, 0.5f, 2.5f, 2},
    // One voice: each note fades out the last, which restarts its phases
    // at every note-on and never falls silent.
    {0, {16, 16}, true, 1200, -1.0f, 0.0f, 2.2f, 2.2f, 1},
    // Three voices of two patches, each note in the releases of the last two.
    {2, {0, 3}, true, 1000, 0.0f, 0.01f, 3.5f, 2.5f, 3},
};

int main(int argc, char** argv) {
  if (argc < 2) return 1;
  FILE* f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  static SixOp engine;
  int index = 0;
  for (const Case& c : kCases) {
    Random::Seed(0x5eed + index);
    engine.Init();
    for (int n = 0; n < kSamples; ++n) {
      const int note = n / c.note_length;
      const int position = n % c.note_length;
      const float pitch_volts = c.note + 0.25f * float(note % 3) +
                                c.vibrato * sinf(float(n) * 0.003f);
      const float pitch = kC4 * powf(2.0f, pitch_volts) / kRate;
      const float envelope =
          c.envelope < 0.0f ? 2.5f + 2.6f * sinf(float(n) * 0.0011f) : c.envelope;
      const float brightness =
          c.brightness < 0.0f ? 2.5f + 2.6f * sinf(float(n) * 0.0007f + 1.0f)
                              : c.brightness;
      const float accent = 1.5f + float(note % 4) * 0.9f;
      const int gate =
          c.gated ? (position < c.note_length * 3 / 4 ? 1 : 0) : -1;
      const int patch = c.patch[note % 2];
      float record[10] = {float(index), pitch, brightness, envelope, accent,
                          float(gate), float(c.bank), float(patch),
                          float(c.voices), 0.0f};
      record[9] = engine.Render(pitch, knob(brightness), knob(envelope),
                                knob(accent), gate, c.bank, patch, c.voices);
      fwrite(record, sizeof(float), 10, f);
    }
    ++index;
  }
  fclose(f);
  return 0;
}
