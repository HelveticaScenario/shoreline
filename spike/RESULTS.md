# Spike: CodeMirror 6 + TypeScript 7.1 over IPC

Branch `spike/codemirror-ts7`. Not for merge. Run with:

```bash
MODULAR_EDITOR=codemirror yarn start
```

Without the variable the app runs Monaco as before.

## What was built

- **Main process**: `src/main/languageService/`
    - `languageServer.ts` spawns TypeScript 7.1's native server (`tsc --lsp --stdio`) in a project directory under `userData` holding only `tsconfig.json`, the DSL declarations and the content mapper. Buffers are virtual `.sl` documents; nothing of the user's is written there.
    - `slContentMapper.ts` is a TypeScript content mapper (its own process). It hands TypeScript the patch verbatim plus one declaration per track label, and maps each declared name back onto the name inside its label.
    - `jsonRpc.ts` is the `Content-Length` JSON-RPC transport both speak.
- **Renderer**: `src/renderer/components/codemirror/`
    - `CodeMirrorPatchEditor.tsx`: one editor, one `EditorState` per buffer (undo, selection, scroll kept per buffer).
    - `languageFeatures.ts`: completion with lazy docs, hover, diagnostics, signature help, occurrence highlights.
    - `navigation.ts` + `peek.ts`: go to / peek definition, peek references, rename box, fix menu, Format Document, Cmd+click.
    - `theme.ts`: app theme (TextMate scopes) to CodeMirror highlight tags and editor chrome.
    - `scopeZones.ts`: scopes as block widgets under their calls.
    - `bufferModel.ts`: a Monaco-text-model-shaped adapter, so the existing step-highlight tracker (`moduleStateTracking.ts`) and scope anchoring (`patchUi.ts`) run on CodeMirror unchanged.
- `PatchEditor.tsx` picks the editor.

## Results against the four spike questions

### 1. Language service: pass

Measured in the app on a 49-line, 12-track patch, 25 edits each, macOS arm64, dev build (`scripts/app/perf.mjs`):

| After an edit (median / p95 ms) | CodeMirror + TS 7.1 over IPC | Monaco + TS worker |
| ------------------------------- | ---------------------------- | ------------------ |
| Diagnostics                     | 8.6 / 11.3                   | 18.0 / 35.2        |
| Completion                      | 5.7 / 7.1                    | 1.3 / 5.5          |
| Hover                           | 0.6 / 2.0                    | 0.6 / 1.1          |

Editor visible after clicking a file: 93 ms against 179 ms. Renderer process working set: about 680 MB against 1150 MB, though the CodeMirror run still loads Monaco's code (not its TS worker). The server process is 41 MB and the mapper process 112 MB (it bundles the TypeScript 5.9 parser).

### 2. Live decorations: pass

Scope panel and playing-step highlights work and follow edits. Frame times while a patch plays with three scopes, and while also typing: 120 fps in both editors, median 8.3 ms, p95 9.1–9.3 ms, no long tasks. One 290 ms frame appeared in one CodeMirror run out of three and did not repeat.

### 3. Theme fidelity: your call

`shots/theme-<theme>-monaco.png` against `shots/theme-<theme>-codemirror.png` for all six bundled themes. They differ, and not because CodeMirror loses anything: Monaco today only colours keywords, strings and comments (its theme conversion drops every dotted scope), while the CodeMirror mapping also colours numbers, function calls, properties and labels as the VS Code themes intend. Missing on the CodeMirror side: rainbow brackets, indent guides.

### 4. Missing UI: prototypes work

Peek (definition and references), rename box, fix menu: `shots/cm-04` to `cm-07`. Functional, unstyled beyond the basics.

## Content mappers

They replace nearly all of `trackLabelBindings.ts`'s position plumbing. With the mapper, and no client-side remapping at all:

- hover, definition, references, highlights and rename on a label's name reach the track's declaration and come back as patch positions;
- renaming from a group suffix (`kick$drums`) renames the group track;
- duplicate-track and other label errors arrive as the mapper's own diagnostics (source `shoreline`);
- errors in the generated declarations are suppressed with an ignore directive;
- unsaved buffers and documents that do not exist on disk go through the mapper.

Still client-side: hiding the `__` names from completion, and rename on a label's prefix offering the whole label.

## Output inlay hints

Not wired into the app. Checked in a script instead (`scripts/ts7/signal.mts`): the existing 1000-line `collectSignalSites` runs unchanged on TypeScript 7.1's sync checker API through a ~30-line adapter and returns results identical to TypeScript 5.9 on two patches (4 and 24 sites). Cost on the 49-line patch: 45 ms and 1416 checker round trips, against 1.5 ms in-process.

Two catches: the sync client cannot attach to the language server's program in 7.1 beta ("Socket connections are not yet supported in the sync client"), so it needs its own TypeScript process; and TypeScript 7's AST has no `GlobalAugmentation` flag, so the collector's `declare global` check needs a one-line change.

## Gaps found

- TypeScript 7.1 beta has no extract / inline refactors and few quick fixes.
- 7.1 is a beta; stable is 7.0.2, which has no content mappers.
- The server binary is 27 MB per platform and must ship unpacked; the mapper needs a Node runtime (the spike runs Electron as Node, which the packaged app's fuses disable).
- Not built: JSON buffers with schema, save-conflict diff, performance window, mixer / EQ / control write-back, video preview panels, the VS Code command-id keymap, context menu.
- The app's integrations take a Monaco editor instance. The adapter approach in `bufferModel.ts` covered scopes and step highlights; the rest need the editor-agnostic buffer interface proposed earlier.
