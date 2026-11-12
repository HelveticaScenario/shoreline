# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

"Shoreline" — a real-time modular synthesizer desktop app with a JavaScript DSL for live-coding audio patches. Electron + React frontend, Rust DSP engine exposed via N-API.

## Commands

### Setup

Repo uses git submodules for vendored Rust DSP crates. Required after clone:

```bash
git submodule update --init --recursive
```

Skipping this produces `failed to load manifest for workspace member` on the first cargo/yarn build.

```bash
yarn install               # Install dependencies
yarn start                 # Build native Rust module + launch Electron app (watches Rust changes)
yarn build-native          # Build native Rust module only
yarn generate-lib          # Regenerate DSL TypeScript types after Rust N-API type changes
yarn typecheck             # TypeScript type checking
yarn lint                  # oxlint on src (also runs on pre-commit via husky)
yarn lint:fix              # oxlint --fix
```

### Testing

| Change type            | Test command                                     |
| ---------------------- | ------------------------------------------------ |
| Rust DSP modules       | `cargo test -p modular_core` or `yarn test:rust` |
| DSL factories/executor | `yarn test:unit`                                 |
| N-API bindings         | `yarn test:unit`                                 |
| Renderer UI/UX         | `yarn test:e2e`                                  |
| Everything             | `yarn test:all`                                  |

E2E tests require the Vite main/preload bundles (`.vite/build`) to exist — run `yarn start` once first.

### Launching the app as an agent

Always set `MODULAR_BACKGROUND_LAUNCH=1` when launching the app (`yarn start`, Playwright, or the built entry): the app never takes focus, its windows open behind the user's, and it runs on a throwaway profile so it never touches the user's running instance. Set `MODULAR_AUDIO_OUTPUT_DEVICE` / `MODULAR_AUDIO_INPUT_DEVICE` to a loopback device id (e.g. `coreaudio:BlackHole64ch_UID`) so test audio stays off the speakers and microphone. `yarn test:e2e` sets all three. See the `verify` skill for driving the app.

## Architecture

### Data Flow

1. **DSL execution** — JavaScript code runs in a `node:vm` context with an execution timeout in `src/main/dsl/executor.ts`, producing a `PatchGraph` JSON structure.
2. **IPC transport** — PatchGraph sent from renderer to main process over Electron IPC (channels defined in `src/shared/ipcTypes.ts`), which calls `synthesizer.updatePatch(graph)`.
3. **Validation** — Rust validates the graph on the main thread (`crates/modular/src/validation.rs`).
4. **Audio thread** — Applied via lock-free command queue (rtrb SPSC) to the audio thread (`crates/modular/src/audio.rs`). Modules instantiated and processed here.
5. **Scope data** — Audio thread writes to ring buffer; renderer polls via `get_scopes()` N-API call; Monaco editor draws oscilloscope overlays.

### Crate Structure

- **`crates/modular_core/`** — Pure Rust DSP library: module trait (`Sampleable`), types (`types.rs`), patch graph (`patch.rs`), all DSP modules (`dsp/`), pattern system.
- **`crates/modular/`** — N-API bindings (`lib.rs`), audio callback (`audio.rs`), validation (`validation.rs`), MIDI input (`midi.rs`), command queue (`commands.rs`).
- **`crates/modular_derive/`** — Proc macros for the module output system.
- **`crates/mi-plaits-dsp-rs/`** — Mutable Instruments Plaits DSP port (git submodule, third-party).

### Frontend Structure

- **`src/main/`** — Electron main process (`main.ts` checks the CPU can run the addon, then loads `app.ts`), DSL runtime (`dsl/executor.ts`, `dsl/factories.ts`, `dsl/GraphBuilder.ts`).
- **`src/renderer/`** — React app (`App.tsx`), Monaco editor (`components/MonacoPatchEditor.tsx`, `components/monaco/`), UI components.
- **`src/preload/`** — Electron context isolation bridge.
- **`src/shared/`** — Shared IPC types.

## Critical Safety Rules

### Thread Safety (violating these causes UB or crashes)

- **NEVER** call `Sampleable` trait methods from the main thread.
- **NEVER** clone module `Arc`s and send them across threads.
- **NEVER** access `Patch::sampleables` from outside `AudioProcessor`.
- **ALWAYS** use the command queue for main-to-audio communication.

### Real-Time Audio Thread

- **No heap allocation on the audio thread.** No `Vec::new`, `Box::new`, `String`, `HashMap`, `.clone()` of heap types in `process` or anything it calls.
- **Allocate in `init` or param deserialization** (both run on the main thread). `process` should only operate on pre-allocated memory.
- Store initialized data on the params struct with `#[serde(skip)] #[schemars(skip)]` for fields hidden from serialization.
- Once deserialization is complete, treat the params object as immutable — `process` reads but never mutates it.

### Module Lifecycle Hooks (`init` vs `on_patch_update` vs `update`)

A module has three places to put logic. Pick by **how often the inputs change** and **what's resolved yet**. Lifecycle order on every patch update is:

```
init (construct, main thread) → transfer_state_from → connect → on_patch_update → … → update (per sample)
```

| Hook                                     | Opt-in                                          | When/thread                                                                          | Put here                                                                                                                                                                                             |
| ---------------------------------------- | ----------------------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fn init(&mut self, sample_rate: f32)`   | `has_init` flag                                 | Once at construction, main thread, **may allocate**                                  | Heap allocation; one-time seeding of runtime state (phases, RNG); **sample-rate-only** derived constants.                                                                                            |
| `fn on_patch_update(&mut self)`          | `patch_update` flag + `impl PatchUpdateHandler` | After every patch update, once all modules are connected, audio thread, **no alloc** | **Param-derived** constant caches; anything needing the resolved graph (connected modules, `Wav::sample_rate()` — resolves in `connect()`). No `sample_rate` arg, so capture it in `init` if needed. |
| `fn update(&mut self, sample_rate: f32)` | —                                               | Every sample, audio thread, hot path, **no alloc**                                   | Only work that reads a live signal (`PolySignal::get_value`/`value_or` — may be a cable varying per sample) or evolving state. Never recompute lifetime-constant work here — hoist it.               |

**Why param-derived caches must NOT go in `init`** (the clobber rule): on a patch update every module is reconstructed, then `transfer_state_from` does `std::mem::swap` on the **entire `state` struct** to preserve runtime continuity — overwriting anything `init` computed into `state`. So a value derived from a non-signal param (recomputed when that param changes) must be set in `on_patch_update`, which runs _after_ the swap and reads the current `self.params`.

**Why sample-rate-only caches are fine in `init`:** a transfer only happens between two modules at the same engine rate (a rate change rebuilds the processor — it is not a state transfer), so the swapped-in old value equals what `init` would compute. `init` also runs **before `connect()`**, so it cannot read connection/topology state.

Examples: `supersaw` — `init` seeds phases + `inv_sample_rate` (sr-only); `on_patch_update` computes `voice_t`/`gain`/`voices` (param-derived). `sampler` — `rate_ratio` is in `on_patch_update` because `wav.sample_rate()` only resolves in `connect()`. Test harnesses that build a module directly must mirror this: call `init` then `on_patch_update` before driving `update`.

**Block modules (`#[module(block)]`).** The module keeps its per-sample `update` and adds `fn process_block(&mut self, outputs: &mut XBlockOutputs, range: Range<usize>, sample_rate: f32) -> Option<()>`. The wrapper runs `process_block` in Block mode and `update` in Sample mode (feedback cycles). Put the per-sample math in one shared function (or a `ChannelState::step`) that both call, so the paths cannot drift; each block module's `tests/block_<module>.rs` checks its paths agree bit for bit and stay allocation-free, using the harness in `tests/block_common`. `process_block` resolves each input once per range and writes straight into the block buffers. `#[derive(BlockInputs)]` on the params struct generates `params.block_inputs(range, channels, &mut scratch)`, which resolves every `PolySignal` and `Option<PolySignal>` field at once; a disconnected optional input reads as its `#[input(default)]`, which `#[params]` also exposes as `Params::<FIELD>_DEFAULT` for `update`'s `value_or`, or as `None` with `#[block_input(optional)]` (an input whose absence changes behaviour, like `sync`). Inputs resolved at other channel counts use `PolySignal::block_input` and `block_input_or` directly — `block_input::map` for elementwise math. `process_block` must not read `self.outputs` or `current_block_index()`: the wrapper updates neither in Block mode. Return `None` (before writing anything) to fall back: the wrapper retries shorter sub-ranges, then `update` slot by slot. Pays off where per-sample input reads dominate (cheap modules like `$amp`) or where per-channel work can be hoisted out of the slot loop for a constant input; not where the cost is per-sample memory reads, such as `$grains`. Measure with `cargo bench -p modular_core --bench patch_bench` and real patches before converting.

### Detecting audio-thread allocations (dev-only)

`yarn build-native-alloc` builds the rust code with a runtime allocation detector compiled in (`--features=alloc-detector` on `crates/modular`). It installs a `#[global_allocator]` that flags any heap allocation/deallocation made on the audio thread and writes a warning to **stderr** — the `rust`-labelled stream in the `concurrently` output — naming the offending module, e.g.:

```
[alloc-detector] AUDIO-THREAD ALLOC in module "osc_3" — 48 bytes (×127 since last report). Move allocation out of process()/update() into init()/on_patch_update() (see CLAUDE.md lifecycle rules).
```

Use it to verify the "no heap allocation on the audio thread" rule above. Notes:

- **Opt-in only.** Plain `yarn build-native` never compiles the detector in (zero cost, byte-identical binary). The detector installs a process-wide global allocator — never enable the feature in a shipped build.
- **Auto-attribution.** Module profiling is force-enabled so attribution works with the editor profiler closed. Allocations on the audio thread but outside any module/scope frame report as `"<unknown>"`.
- **Detect-and-flag, never fail.** The real `System` allocation always runs first; the detector only records. All formatting/logging happens on a background thread, never the audio thread. Dropped/dealloc events and running totals are summarized periodically on the same `[alloc-detector]` stderr stream.

## Conventions

### Voltage Standards

- **V/Oct pitch**: 1V/oct, 0V = C4 (~261.63 Hz).
- **Gates/triggers**: `GATE_HIGH_VOLTAGE` (5V) high, `GATE_LOW_VOLTAGE` (0V) low. Constants in `crates/modular_core/src/dsp/utils/` (module directory).
- **Gate detection**: Schmitt trigger with hysteresis — high threshold 1.0V, low threshold 0.1V. Use `SchmittTrigger::default()`.
- **Output attenuation**: `AUDIO_OUTPUT_ATTENUATION` in `crates/modular/src/audio.rs`.

### Adding/Changing Module Params

1. Update param structs + DSP in `crates/modular_core/src/dsp/**/*.rs`.
    - Put `#[params]` above the params struct's `#[derive(...)]` and document every field with `#[input("description", …)]` (never `///`). Signal fields need `type` (`audio`, `control`, `pitch`, `frequency`, `gate`, `trig`). `range` is the usable range shown in the docs: values past it are clamped, rejected or do nothing more, and either side may be open (`(0.0, ..)`). `control_range` is a control's slider span, required when `range` is open. A default that depends on other params adds `default_doc`, the default in words (`"0 if decay is set, otherwise 5"`), which the docs show in place of the number. Outputs are documented only in `#[output("name", "description", type = …)]`, where `type` is `cv`, `gate` or `trig`.
    - The docs list params and outputs in declaration order, so declare them in the order a musician reads them, positional params first in call order (`tests/input_metadata.rs` checks the positionals).
    - Descriptions are short and musical. They don't restate what the editor already shows beside them: the range, the default or the V/Oct unit.
2. Wire schema/validators in category modules (e.g., `oscillators/mod.rs` via `install_constructors` / `install_param_validators`).
3. Rebuild N-API (`yarn build-native`) to refresh `crates/modular/index.d.ts`.
4. Adjust DSL factories in `src/main/dsl/factories.ts` if needed.

A param whose edits would click (a gain, a cutoff, a mix amount) glides across patch updates: keep a `SwapGlide` in `state`/`channel_state` and feed it with `glide.update(value, param.glide_mark(ch), sample_rate)`. It glides only when that param's source changed in the swap, so live modulation and unchanged cables pass through untouched; no hook is needed. The per-sample check still costs a few operations per channel, which shows on modules that do little else (`$signal`, `$curve`): give those a block path that returns `None` while `param.glides_pending(glides)` and otherwise reads the param raw and calls `param.settle_glides`, and use `glide.follow` in `update` so both paths agree.

**Bounds.** An output can carry bounds: the two values its signal swings between, in order (`.range(5, 0)` is `(5, 0)`), stored per channel per slot beside its samples and read through the same paths, so a reader always sees the bounds of the sample it read (`PolySignal::get_bounds`, `block_bounds`). An output with a `range` (and every gate or trigger) is bounded by it. `#[output(…, bounds = params)]` declares one the module bounds from its own params (`$remap`); `bounds = input` one whose bounds come from the inputs tagged `#[input(…, bounds)]`, bounded when they all are (the operators, `$round`, `$slew`); `#[input(…, bounds = constant)]` tags one that must also hold a constant (`$div`'s divisor), and `#[params(declares_range = (min, max))]` also bounds them by two optional params whenever both are set (`$clamp`, and `$signal`, whose range holds where its value has none); the schema states all of this, so the DSL types a result ranged by the same rule. `#[params(bounded_when = …)]` replaces the test with one the schema cannot state (`$erratic`), and `#[params(reads_bounds = …)]` names inputs the module reads bounds from for itself (`$remap`'s unset input range), and `settles = …` fixes, once per patch, a choice made from which inputs are bounded (`$erratic`'s unset mode). `Patch::settle_bounds` settles which outputs are bounded when the patch is built, so a feedback cycle that only feeds itself stays unbounded, and gives storage only to bounded outputs something reads. A module whose output stores bounds writes them with every sample: `PolyOutput::set_bounds` in `update` when `stores_bounds()`, and in `process_block`, before the samples, one row via `block_bounds` and `fill_bounds_with`, or per slot via `block_input::map_bounds`; these decline, writing nothing, when an input's bounds move within the range, and a debug build asserts no render leaves bounds unwritten. A monotonic rule maps the input's bounds through the module's own sample function (`Bounds::map`, `Bounds::combine`), so the two cannot drift; `tests/bounds.rs` checks lock step on every read path, and each ruled module's `block_<module>.rs` checks its bounds paths agree with `check_bounds_agree`.

### Module Docs

A module's reference docs are one `const NAME_DOCS: ModuleDocs` beside it (`crates/modular_core/src/module_docs.rs`), named in `#[module(name = …, docs = NAME_DOCS)]`. A `///` on a `#[module]` struct is a compile error. It holds:

- `summary`: 1–6 words;
- `description`: 1–3 sentences of inline markdown, never explaining individual params, except to flag a param behaviour surprising enough to need a prominent note (`$adsr`'s sustain);
- `categories`;
- `sections`: titled body, `DocTable` and examples, only for real reference material such as a syntax guide;
- `examples`: titled patches that run as written, formatted as the editor's Format Document prints them (Prettier with `DEFAULT_PRETTIER_OPTIONS`). A multi-line example is an indented raw string opening on its own line; the schema strips the indent.

Monaco shows the description, sections and first example; the Reference pane shows everything. `tests/module_docs.rs` checks the shape, and `generatedExampleValidity.test.ts` runs every example, and `referenceDocs.test.ts` checks each is formatted.

### Experimental Modules

`#[module(name = "$comp", docs = COMP_DOCS, args(input), experimental)]` marks a module experimental; the editor's hover, completion and signature help, and the Reference pane tell users its sound and params may still change, so its `ModuleDocs` never says so itself.

- **Mark a module experimental** when any of these hold: its API (name, params, ranges, outputs) isn't settled; its sound hasn't been validated against a reference or by listening; a planned feature would change its param shape; it is one of several competing candidates.
- **Not a reason:** failing the alloc audit or another engine rule. A module that breaks those never merges.
- **Graduate** by deleting the flag (the name stays, so patches keep working) once param conventions are followed (0–5 knob ranges, V/Oct), docs and runnable examples exist, tests exist, the API has been stable for a while, and the user signs off explicitly.
- Changes to an experimental module's API or sound are still breaking changes: changelog kind `Breaking`.
- `yarn experimental:report` lists experimental modules and flags those unchanged for over 60 days as overdue; every release PR includes it.

### Reserved Member Names

When adding methods to `BaseCollection`, `Collection`, `CollectionWithRange`, `DeferredCollection`, or patterns (`PATTERN_METHODS` in `src/main/dsl/miniNotation/index.ts`), add the method name to `RESERVED_MEMBER_NAMES` in `crates/reserved_member_names.rs`. This is the single source of truth shared by the Rust proc-macro and, via NAPI, the TypeScript tests that check the list against the classes.

The list limits module names. Every module whose first argument is a signal is a chain method of collections (`$lpf` as `sig.lpf(...)`, `$mi.chorus` as `sig.mi.chorus(...)`), so the first segment of a module's name after its `$` cannot be reserved: `#[module]` rejects `$out` and `$out.soft`. A pattern is the `$cycle` that plays it and has every collection member beside its own methods, so those names are reserved too; `$cycle` itself is exempt, as the module a pattern's `.cycle(...)` calls. A module's outputs sit behind an underscore (`seq._gate`), so an output may take any name. A module the DSL exposes starts with `$`, an internal one with `_`.

### Code Organization

- Break files over ~400 lines into smaller domain-specific modules.
- Patch graphs are the contract — update Rust types in `modular_core::types`, not TypeScript.
- Prefer Electron APIs over web/React APIs when either could solve a task.

### Comments

**The codebase is a present-tense artifact. It must read as though it was written this way from the start.** This is a hard invariant, the single most important rule here — never relaxed, no exceptions:

- **No history, anywhere.** Never reference commits, PRs, tickets, or past states — no "used to", "now", "previously", "formerly", "renamed", "instead of", "we changed", "this was". History lives in git, and only in git.
- **No roads not taken.** Never mention alternatives considered, decisions made, approaches rejected, or bugs once present. The current code is the only thing that exists; comments describe what must be true, never what once was. (A regression test states the invariant it guards, not the bug it fixed.)

Within that invariant, comment well:

- **Only the non-obvious.** Explain a constraint, invariant, unit, or consequence a reader needs to use the code correctly. If the code is clear, leave it uncommented.
- **Short and complete.** One or two full sentences. Cut jargon and detail the reader doesn't need.
- **No redundant restatement.** If a comment re-says the line below it, delete it.
- **Don't over-comment.** Match the surrounding file's density. Spend comments on safety/thread-safety/real-time rules and boundary semantics (IPC, DSL, Electron quirks).
- **Leave nothing dead.** No commented-out code; no unowned `TODO`/`FIXME`.

Models to emulate: `crates/modular_core/src/types.rs` (safety invariants), `src/main/bufferSwitch.ts` (boundary semantics).

### Changelog

Every PR that changes something a user can notice adds a release-note entry: `yarn changelog:new -k <Kind> -b "<text>"`, which writes `.changes/unreleased/*.yaml`. CI fails a PR that changes `src/` or `crates/` without one unless it is labelled `no-changelog` (refactors, internal tooling, test-only work). Users read these in the app before deciding to update, so write for musicians, not developers:

- **Kinds:** `Breaking` (a patch that ran on the last release can sound or behave differently, or stop running — includes any change to an experimental module's API or sound), `New`, `Improved`, `Fixed`. `Breaking` bumps the minor version (pre-1.0); the rest bump the patch.
- **Write against the last release, not the last PR.** A release's notes describe what changed since the previous published version, read as one list. Before adding an entry, search `.changes/unreleased/` for the names your change touches (`grep -rn mixDown .changes/unreleased`): if an entry already covers that feature, edit it so it describes the result instead of adding one. Changing something users never had is not `Breaking`; renaming it rewrites the existing entry's names and examples, and removing it deletes the entry. CI accepts an edited or deleted entry the same as a new one.
- **Say what the user can now do or what changes in their patch.** Name modules and params as they appear in the DSL (`$comp`, `attack`). No internals: no crate, file, struct, thread, or refactor names.
- **One entry per change, one or two sentences.** Several entries in one PR are fine when it ships several user-visible changes.
- **Every `Breaking` entry shows the migration**: a short before/after code block of a patch.

Entries are reviewed in the PR diff; the release PR collects them. Before merging the release PR, read its notes as a whole and merge or reword entries that overlap.

### Releasing

Nothing is committed to master except through a PR. On every push to master, the Release PR workflow runs `scripts/release/prepare.mjs` and keeps a `Release vX.Y.Z` PR (branch `release/next`) up to date: it batches the pending entries into `.changes/vX.Y.Z.md`, regenerates `CHANGELOG.md`, and sets `package.json`'s version. Merging that PR ships the release: `release.yml` creates a draft GitHub release whose notes are that version's entries, builds every platform into it, and publishes it once all uploads succeed. To force a specific version (e.g. `v1.0.0`), commit `.changes/release-as` containing it; the next prepare consumes it.

## Tooling

- **Node 24.12.0 / Yarn 4.12.0** pinned via Volta.
- **Rust edition 2024**.
- **Release targets**: macOS arm64 + x64, Windows x64, Linux x64. x86_64 builds assume `x86-64-v2` (root `.cargo/config.toml`), so benchmark that build, never `-C target-cpu=native`.
- **sccache** (when it is the rustc wrapper) runs in client-side mode via `[env]` in the root `.cargo/config.toml`, so killing a build cannot drain the shared server's jobserver tokens. Keep that setting; sccache ignores it while `SCCACHE_ERROR_LOG` is set. If builds still sit at 0% CPU on every worktree, run `sccache --stop-server`, which restores the pool.
- **Husky pre-commit hooks** run Prettier on `*.{ts,tsx,js,jsx,mjs,json,css,md}` and `cargo fmt` on Rust files.
- **Vitest** for JS/TS tests, **Playwright** for E2E, **cargo test** for Rust.
