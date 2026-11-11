// Plaits' six-operator FM engine (six_op_engine.cc + fm/voice.h) with one
// voice, which every note-on retriggers, and Operator's control model:
// SixOpEngine's logic for its active voice and that fm::Voice<6>'s
// envelopes, LFO and level scaling run every sample, and the operators render
// through Plaits' own compiled render calls one sample at a time. The pitch
// is exact: the note's frequency times 2^(pitch modulation), the note number
// for keyboard and rate scaling taken from that frequency. A patch change
// renders at once. Each case is a fresh engine from its own random seed. Each
// record is one sample's inputs (case, pitch, brightness, envelope, accent,
// gate as 1, 0 or -1 for unpatched, bank, patch) then the engine's
// soft-clipped output, before the output gain.
// build: stmlib/dsp/units.cc stmlib/utils/random.cc plaits/dsp/engine2/six_op_engine.cc plaits/dsp/fm/algorithms.cc plaits/dsp/fm/dx_units.cc

#include <algorithm>
#include <cmath>
#include <cstdio>

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

static float knob(float volts) {
  float v = volts * 0.2f;
  return v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
}

static float MidiNote(float f0) {
  double note = 69.0 + 12.0 * log2(double(fabsf(f0)) * double(kRate) / 440.0);
  return std::min(std::max(float(note), -119.0f), 120.0f);
}

// fm::Voice<6>::Render for one sample, with an exact pitch `f0` (cycles per
// sample) and no blank sample after a patch change.
static void RenderVoice(Voice<6>& v, const Voice<6>::Parameters& p, float f0_in,
                        float* out) {
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
    a[i] = Pow2Fast<2>(-14.0f + level * level_mod);
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

struct SixOp {
  Algorithms<6> algorithms;
  Patch patches[3][32];
  FMVoice voice;
  bool gate;

  void Init() {
    algorithms.Init();
    voice.Init(&algorithms, kRate);
    for (int b = 0; b < 3; ++b) {
      for (int i = 0; i < 32; ++i) {
        patches[b][i].Unpack(fm_patches_table[b] + i * Patch::SYX_SIZE);
      }
    }
    gate = false;
  }

  // SixOpEngine::Render for one sample with one voice, which every note-on
  // takes; `gate` < 0 is unpatched.
  float Render(float pitch, float brightness, float envelope, float accent,
               int gate_in, int bank, int patch_index) {
    const Patch* patch = &patches[bank][patch_index];
    Voice<6>::Parameters* p = voice.mutable_parameters();
    if (gate_in < 0) {
      voice.mutable_lfo()->Scrub(2.0f * kRate * envelope);
      voice.LoadPatch(patch);
      p->sustain = true;
      p->gate = false;
    } else {
      const bool high = gate_in > 0;
      if (high && !gate) {
        voice.LoadPatch(patch);
        voice.mutable_lfo()->Reset();
      }
      gate = high;
      voice.mutable_lfo()->Step(1.0f);
      p->sustain = false;
      p->gate = high;
    }
    p->velocity = accent;
    p->brightness = brightness;
    p->envelope_control = envelope;
    voice.set_modulations(voice.lfo());
    float out = 0.0f;
    if (voice.patch_) RenderVoice(voice.voice_, voice.parameters_, pitch, &out);
    return SoftClip(out * 0.25f);
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
};

static const Case kCases[] = {
    // Three-operator feedback loop (algorithm 4), pitch envelope, sine LFO.
    {0, {26, 26}, true, 3200, -1.0f, 0.0f, 2.5f, -1.0f},
    // Pitch envelope and delayed LFO, retriggered fast, envelope sweeping.
    {1, {13, 13}, true, 1200, 0.25f, 0.01f, -1.0f, 2.0f},
    // Two-operator feedback loop (algorithm 6), fixed-frequency operators,
    // slow envelopes.
    {1, {25, 25}, true, 1600, -0.5f, 0.0f, 4.5f, 3.5f},
    // Scrubbed: amplitude LFO, envelope and brightness sweeping.
    {2, {11, 11}, false, 8000, -1.5f, 0.0f, -1.0f, -1.0f},
    // Scrubbed: fixed-frequency operators, pitch envelope, gliding pitch.
    {2, {4, 4}, false, 8000, 0.0f, 0.3f, 1.0f, 2.5f},
    // Notes alternating between two patches, each retriggering in the
    // other's release.
    {2, {0, 3}, true, 2000, 0.5f, 0.0f, 3.0f, -1.0f},
    // Snappy envelopes, retriggered in their releases.
    {0, {18, 9}, true, 2600, -2.0f, 0.02f, 0.5f, 2.5f},
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
      float record[9] = {float(index), pitch, brightness, envelope, accent,
                         float(gate), float(c.bank), float(patch), 0.0f};
      record[8] = engine.Render(pitch, knob(brightness), knob(envelope),
                                knob(accent), gate, c.bank, patch);
      fwrite(record, sizeof(float), 9, f);
    }
    ++index;
  }
  fclose(f);
  return 0;
}
