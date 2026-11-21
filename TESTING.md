# Testing Guide

Shoreline is an Electron app (React/TypeScript renderer) on a Rust DSP engine. Tests run in three layers.

| Change type            | Test command                                     |
| ---------------------- | ------------------------------------------------ |
| Rust DSP modules       | `cargo test -p modular_core` or `yarn test:rust` |
| DSL factories/executor | `yarn test:unit`                                 |
| N-API bindings         | `yarn test:unit`                                 |
| Renderer UI/UX         | `yarn test:e2e`                                  |
| Everything             | `yarn test:all`                                  |

## Rust tests

- **Command:** `yarn test:rust` generates the Strudel parity fixtures (`yarn gen:sp-fixtures`, which the `sp_combine` tests read) and runs `cargo test` for the workspace. `cargo test -p modular_core` runs the engine tests alone once the fixtures exist.
- **Location:** unit tests sit beside the code in `crates/modular_core/src/`; integration tests are in `crates/modular_core/tests/`. Notable suites:
    - `dsp_fresh_tests.rs` — every module constructs, validates its params and renders. A module with a required param needs a `minimal_params` case here.
    - `block_<module>.rs` — a block module's Block and Sample paths agree bit for bit and stay allocation-free, using the harness in `tests/block_common`.
    - `bounds.rs` — output bounds stay in lock step with their samples on every read path.
    - `module_docs.rs`, `input_metadata.rs` — module docs and param metadata have the required shape and order.
    - `swap_glides.rs`, `connect_alloc_free.rs` — patch updates glide changed params and allocate nothing on the audio thread.
    - `peggy_parser_parity.rs`, `sp_combine_strudel_parity.rs` — the mini-notation parser and pattern combinators match their JS references.
- Multi-module patches are built with `Patch::from_graph()`, a test-only helper, so no audio thread is needed.
- **Benchmarks:** `cargo bench -p modular_core --bench patch_bench` (also `drums_bench`, `mix_bench`, `overdrive_bench`, `shape_bench`).

## Vitest unit and integration tests

- **Command:** `yarn test:unit` (`vitest run`). Requires the native module (`yarn build-native`).
- **Config:** `vitest.config.ts`. Tests run in a single forked process because the N-API module cannot load in worker threads.
- **Location:** `src/**/*.test.ts(x)`, mostly in `__tests__` folders beside the code, and `crates/modular/__test__/napi.test.ts` for the N-API boundary. Notable suites:
    - `src/main/dsl/__tests__/executor.test.ts` — DSL source to `PatchGraph`, without Electron.
    - `generatedExampleValidity.test.ts` — every module docs example runs as a valid patch.
    - `referenceDocs.test.ts` — every docs example is formatted as Format Document prints it.
- **Fixtures:** `yarn gen:dsl-ranged-fixture` and `yarn gen:parser-parity-fixture` regenerate the fixtures the Rust tests compare against. CI fails if the committed parser-parity fixture differs from a fresh run.

## Playwright E2E tests

- **Command:** `yarn test:e2e` (`playwright test`).
- **Config:** `playwright.config.ts`. Tests run one at a time, since each file launches its own app instance. A Vite dev server serves the renderer on port 5173; a running `yarn start` server is reused.
- **Prerequisite:** the Vite main/preload bundles in `.vite/build`. Run `yarn start` once to create them.
- **Fixtures:** `e2e/fixtures.ts` launches Electron with a temporary workspace and profile, `MODULAR_BACKGROUND_LAUNCH=1`, and audio on the `BlackHole64ch` loopback device, so tests never steal focus, touch your running instance or reach your speakers.
- **Location:** `e2e/*.test.ts` and `e2e/tests/*.test.ts`.
- With `E2E_TEST=1`, the renderer exposes `window.__TEST_API__`, including:
    - `newUntitledFile()` — open a buffer. The test workspace starts with none, and `setEditorValue` and `executePatch` do nothing without one, so call `openUntitledBuffer(window)` from the fixtures first.
    - `getEditorValue()` / `setEditorValue(code)` — read and write the Monaco editor.
    - `executePatch()` / `getLastPatchResult()` — run the patch and read the DSL result, including errors.
    - `getScopeData()`, `getVuMeterData()`, `getAudioHealth()`, `isClockRunning()` — observe audio.
- The agent shell may export `ELECTRON_RUN_AS_NODE`, which breaks Playwright's Electron launch; run `env -u ELECTRON_RUN_AS_NODE yarn test:e2e` if launch fails with `bad option: --remote-debugging-port=0`.

## CI

`.github/workflows/ci.yml` runs on every PR: the native build, `yarn test:unit` and the parser-parity check in one job; `cargo test -p modular_core`, `cargo fmt --check` and clippy in another. E2E tests run locally only.

## Audio-thread allocations

`yarn build-native-alloc` builds the engine with an allocation detector that reports any heap allocation on the audio thread to stderr. See CLAUDE.md for how to read its output.
