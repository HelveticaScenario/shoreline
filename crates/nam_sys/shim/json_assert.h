// Force-included before every NeuralAmpModelerCore source. The runtime reads
// nested model fields with nlohmann's unchecked `operator[]`, which asserts on
// a missing key; this turns that into an exception the shim reports, so a
// malformed .nam file is an error instead of a crash.

#pragma once

#include <stdexcept>

#define JSON_ASSERT(x) ((x) ? static_cast<void>(0) : throw std::runtime_error("malformed .nam file (" #x ")"))
