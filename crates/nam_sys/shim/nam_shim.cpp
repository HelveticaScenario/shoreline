// C ABI over NeuralAmpModelerCore. Every entry point catches all C++
// exceptions: unwinding into Rust is undefined behaviour.

#include <cstddef>
#include <cstring>
#include <exception>
#include <filesystem>
#include <memory>
#include <string>
#include <vector>

#include "NAM/dsp.h"
#include "NAM/get_dsp.h"
#include "NAM/nam_file.h"
#include "NAM/slimmable.h"
#include "json.hpp"

namespace
{

void write_error(char* err, std::size_t err_len, const char* message)
{
  if (err == nullptr || err_len == 0)
    return;
  std::strncpy(err, message, err_len - 1);
  err[err_len - 1] = '\0';
}

} // namespace

// A parsed .nam file, shared read-only by every model built from it.
struct NamFile
{
  nlohmann::json config;
};

// One running model plus the double-precision buffers it reads and writes.
// `process` touches only memory sized here, so it never allocates.
struct NamDsp
{
  std::unique_ptr<nam::DSP> dsp;
  std::vector<NAM_SAMPLE> in;
  std::vector<NAM_SAMPLE> out;
};

extern "C" {

// `path` is UTF-8, `len` bytes long.
NamFile* nam_file_open(const char* path, std::size_t len, char* err, std::size_t err_len)
{
  try
  {
    const std::u8string utf8(reinterpret_cast<const char8_t*>(path), len);
    auto file = std::make_unique<NamFile>();
    file->config = nam::validate_nam_file(std::filesystem::path(utf8));
    nam::verify_config_version(file->config.at("version").get<std::string>());
    return file.release();
  }
  catch (const std::exception& e)
  {
    write_error(err, err_len, e.what());
  }
  catch (...)
  {
    write_error(err, err_len, "unknown error reading .nam file");
  }
  return nullptr;
}

void nam_file_free(NamFile* file)
{
  delete file;
}

// The rate the model was trained at, or -1 when the file does not say.
double nam_file_sample_rate(const NamFile* file)
{
  try
  {
    return nam::get_sample_rate_from_nam_file(file->config);
  }
  catch (...)
  {
    return -1.0;
  }
}

NamDsp* nam_dsp_new(const NamFile* file, double sample_rate, int max_frames, char* err, std::size_t err_len)
{
  try
  {
    nam::DspLoadOptions options;
    options.prewarm = false;
    auto dsp = std::make_unique<NamDsp>();
    dsp->dsp = nam::get_dsp(file->config, options);
    if (dsp->dsp->NumInputChannels() != 1 || dsp->dsp->NumOutputChannels() != 1)
    {
      write_error(err, err_len, "only mono-in, mono-out models are supported");
      return nullptr;
    }
    dsp->in.assign(static_cast<std::size_t>(max_frames), 0.0);
    dsp->out.assign(static_cast<std::size_t>(max_frames), 0.0);
    dsp->dsp->ResetAndPrewarm(sample_rate, max_frames);
    return dsp.release();
  }
  catch (const std::exception& e)
  {
    write_error(err, err_len, e.what());
  }
  catch (...)
  {
    write_error(err, err_len, "unknown error building NAM model");
  }
  return nullptr;
}

void nam_dsp_free(NamDsp* dsp)
{
  delete dsp;
}

// Processes `frames` samples, at most the `max_frames` the model was built
// with. Returns 0 on success; on failure `output` is left untouched.
int nam_dsp_process(NamDsp* dsp, const float* input, float* output, int frames)
{
  try
  {
    if (frames <= 0 || static_cast<std::size_t>(frames) > dsp->in.size())
      return 1;
    for (int i = 0; i < frames; ++i)
      dsp->in[i] = static_cast<NAM_SAMPLE>(input[i]);
    NAM_SAMPLE* in_ptr = dsp->in.data();
    NAM_SAMPLE* out_ptr = dsp->out.data();
    dsp->dsp->process(&in_ptr, &out_ptr, frames);
    for (int i = 0; i < frames; ++i)
      output[i] = static_cast<float>(dsp->out[i]);
    return 0;
  }
  catch (...)
  {
    return 1;
  }
}

// Picks the tier of a model that holds several, from 0 (smallest) to 1
// (largest), and prewarms it. Returns 0 for a model with one tier. Allocates.
int nam_dsp_set_size(NamDsp* dsp, double size)
{
  try
  {
    auto* slimmable = dynamic_cast<nam::SlimmableModel*>(dsp->dsp.get());
    if (slimmable == nullptr)
      return 0;
    slimmable->SetSlimmableSize(size);
    dsp->dsp->prewarm();
    return 1;
  }
  catch (...)
  {
    return 0;
  }
}

// Each level getter returns 1 and writes `value` when the model knows it.
int nam_dsp_loudness(NamDsp* dsp, double* value)
{
  try
  {
    if (!dsp->dsp->HasLoudness())
      return 0;
    *value = dsp->dsp->GetLoudness();
    return 1;
  }
  catch (...)
  {
    return 0;
  }
}

int nam_dsp_input_level(NamDsp* dsp, double* value)
{
  try
  {
    if (!dsp->dsp->HasInputLevel())
      return 0;
    *value = dsp->dsp->GetInputLevel();
    return 1;
  }
  catch (...)
  {
    return 0;
  }
}

} // extern "C"
