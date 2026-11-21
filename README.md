# Shoreline

A real-time modular synthesizer desktop app with a JavaScript DSL for live-coding audio patches, built with Electron and a Rust DSP engine.

## Features

- **JavaScript DSL** - Expressive, fluent API with `$`-prefixed functions for building synthesis patches
- **Real-time Audio** - Low-latency audio processing in Rust via N-API, running directly in-process
- **Live Coding** - Update patches on-the-fly with Alt+Enter
- **Monaco Editor** - Full-featured code editor with autocomplete, inline error highlighting, and oscilloscope overlays
- **UI Sliders** - Bind real-time UI sliders to signal parameters with `$slider()`
- **Module Library** - Oscillators, filters, effects, sequencers, envelopes, and more

## Quick Start

### Prerequisites

- Rust (latest stable)
- Node.js 24.12+
- Yarn (latest)

> **Note:** This project uses [Volta](https://volta.sh/) to manage the Node.js version. If you have Volta installed, it will automatically use the correct version pinned in `package.json`.

### Acquiring submodules

On macOS, the Syphon bridge builds the Syphon framework from a git submodule. Fetch it after cloning:

```bash
git submodule update --init --recursive
```

### Running the App

```bash
yarn install
yarn start
```

This builds the native Rust audio module and launches the Electron app.

## Usage

1. Launch the app with `yarn start`
2. Write a patch using the JavaScript DSL (see examples below)
3. Press **Alt+Enter** to execute and hear the result
4. Press **Alt+.** to stop audio

### Example Patches

**Simple Sine Wave:**

```javascript
tone: $sine('440hz');
```

**Musical Note:**

```javascript
keys: $sine('a3');
```

**FM Synthesis:**

```javascript
const mod = $sine('220hz');
const carrier = $sine('440hz').phase(mod.amp(0.5));
fm: carrier;
```

**Tracks:** a label in front of a statement sends it to the mixer. `_name` mutes it, `S_name` solos it (`S__name` does both, and solo wins), and `name$2` plays it on channels 2 and 3 (`name$2m` in mono on channel 2), and `name$drums` plays it into the group track `drums`, a track that starts from `$group()`. `.track()` sets its gain, pan, spread and sends.

```javascript
verb: $bus().plate().track({ gain: 3 });
bass$0m: $saw('c2').lpf('400hz');
lead: $sine('c5').track({ pan: -2, sends: [verb, 2] });
```

**With a UI Slider:**

```javascript
const shape = $slider('Shape', 0, 0, 5);
keys: $saw('a3', { shape });
```

## Project Structure

```
modular/
├── crates/
│   ├── modular_core/      # Core DSP engine (Rust)
│   ├── modular/           # N-API bindings + audio thread (Rust)
│   └── modular_derive/    # Proc macros for the module system (Rust)
├── src/
│   ├── main/              # Electron main process (TypeScript)
│   │   └── dsl/           # DSL executor, factories, type generation
│   ├── renderer/          # React renderer (TypeScript)
│   ├── preload/           # Electron preload scripts
│   └── shared/            # Shared types between main and renderer
└── docs/                  # Documentation
```

## Architecture

### Rust DSP Engine

- **modular_core** - Audio DSP engine: oscillators, filters, effects, sequencers, envelopes
- **modular** - N-API bindings exposing the engine to Node.js; runs the audio callback via cpal; streams scope data back to the renderer
- **modular_derive** - Proc macros for the module output system

### Electron App (TypeScript/React)

- **DSL Runtime** - Executes JavaScript patches via `new Function(...)`, generates `PatchGraph` JSON
- **Editor** - Monaco-based editor with autocomplete, inline error display, and waveform overlays
- **IPC** - Patch graphs are sent from the renderer to the main process over Electron IPC, which forwards them to the Rust synthesizer

## Development

### Type Generation

After modifying Rust N-API types, regenerate the DSL type definitions:

```bash
yarn generate-lib
```

### Building

```bash
# Build native Rust module only
yarn build-native

# Build and launch the full app
yarn start
```

### Testing

```bash
# Rust DSP unit tests
yarn test:rust

# JavaScript unit/integration tests (Vitest)
yarn test:unit

# End-to-end tests (Playwright + Electron)
yarn test:e2e

# All tests
yarn test:all
```

See [TESTING.md](TESTING.md) for more details on the test infrastructure.

## License

AGPL-3.0
