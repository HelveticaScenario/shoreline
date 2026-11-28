# Moving the patch editor to CodeMirror 6: findings

Everything learned about CodeMirror while spiking a replacement for Monaco, kept for the day a migration is decided. Measurements and screenshots are in `RESULTS.md` and `shots/`; this file covers what CodeMirror can and cannot do for us, how the spike solved each part, and what is left.

No migration has been decided, and no reason for moving has been settled. The go/no-go bar depends on that reason (bundle size, Monaco's workarounds, extensibility, or something else), so weight the sections below accordingly.

## Where things are

- **CodeMirror spike**: branch `spike/codemirror-ts7` (local, not pushed, never for merge), worktree `modular.worktrees/modular/monaco-features-inventory-c4e233`. Run with `MODULAR_EDITOR=codemirror yarn start`.
    - Renderer code: `src/renderer/components/codemirror/`, about 2,400 lines.
    - Driver scripts: `spike/scripts/app/`, untracked because the commit hook's lint rejects them.
- **TypeScript 7.1 language server for Monaco**: branch `spike/ts7-language-server`, worktree `modular.worktrees/modular/ts7-language-server`, cut from master with no CodeMirror code. Run with `MODULAR_LANGUAGE_SERVER=ts7 yarn start`. A CodeMirror migration can reuse its main-process server, content mapper and IPC as they are.

## Verdict from the spike

The spike tested four questions in the running app (dev build, macOS arm64, a 49-line patch with 12 tracks):

| Question                              | Result                                                                                                                                                                                                   |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Language service as fast as Monaco's  | **Pass.** Diagnostics after an edit: 8.6 / 11.3 ms (median / p95) against 18.0 / 35.2. Completion: 5.7 / 7.1 against 1.3 / 5.5. Hover is the same. All of these are server numbers, not CodeMirror ones. |
| Live decorations as cheap as Monaco's | **Pass.** 120 fps in both editors while playing with three scopes and typing (median frame 8.3 ms). One 290 ms frame in one CodeMirror run out of three, which did not repeat.                           |
| Themes look acceptable                | **Needs your judgment.** The colours differ: CodeMirror colours more token kinds than Monaco does today. Rainbow brackets and indent guides are missing.                                                 |
| Peek, rename and fix menu feel right  | **Prototypes work.** They are functional, with basic styling only.                                                                                                                                       |

Two other measurements:

- **Editor visible after clicking a file**: 93 ms against 179 ms.
- **Renderer working set**: about 680 MB against 1,150 MB, even though the CodeMirror run still loads Monaco's code.

Bundle size and cold start were not measured.

## How Monaco features map onto CodeMirror

### Clean mappings

| Monaco today                                                          | CodeMirror 6                                                                                                                                                 |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| One model per buffer, view state saved and restored                   | One `EditorState` per buffer, swapped with `view.setState`. Undo, selection and decorations live in the state; scroll is saved with `view.scrollSnapshot()`. |
| Tracked decorations and anchors                                       | `RangeSet` decorations mapped through changes, and `ChangeSet.mapPos` for bare anchors                                                                       |
| View zones (scopes, video previews, EQ panels)                        | Block widget decorations. `WidgetType.ignoreEvent()` takes the place of the click and context-menu workarounds.                                              |
| Playing-step highlights, code backing                                 | Mark and line decorations from a `StateField`                                                                                                                |
| Completion (TypeScript, MIDI devices, paths, keybindings)             | `@codemirror/autocomplete`, which takes several sources and lazy `info`                                                                                      |
| Diagnostics, next/previous problem                                    | `@codemirror/lint`                                                                                                                                           |
| Hover                                                                 | `hoverTooltip`, rendered with the `react-markdown` we already ship. Doc links become plain click handlers.                                                   |
| Inlay hints                                                           | Inline widget decorations                                                                                                                                    |
| Find and replace, Go to Line                                          | `@codemirror/search`                                                                                                                                         |
| Multi-cursor, line operations, comments, bracket matching, auto-close | `@codemirror/commands`, `@codemirror/autocomplete`, `@codemirror/language`                                                                                   |
| Save-conflict diff                                                    | `@codemirror/merge`                                                                                                                                          |
| `colorize` and `tokenize` (docs snippets, performance view)           | Lezer's `highlightCode` / `highlightTree`, with no editor instance and no deep Monaco imports                                                                |
| Format Document                                                       | The same Prettier code, dispatched as one change                                                                                                             |
| Cmd+click, file drop, native context menu                             | `EditorView.domEventHandlers` and `posAtCoords`                                                                                                              |

### Workarounds that go away

- the hidden hover-delegate anchor editor;
- the "Canceled" error suppression;
- the hand-written types for Monaco internals;
- the DOM observer that tracks suggest and find widget visibility;
- re-registering the inlay-hint provider to force a refresh;
- the sticky-scroll transparent-background issue.

### Pieces with no CodeMirror equivalent

- **Peek definition and references.** The spike built it.
- **Rename box** with a rejection message at the cursor. Built.
- **Code action menu** with lazy resolve and follow-up commands (caret placement, rename after extract). Built without lazy resolve or follow-ups.
- **Signature help.** Built, but with a single-line heuristic (see the spike limitations below).
- **Occurrence highlighting.** Built.
- **Go to Symbol.** Not built; it would go into our own command palette.
- **A table from VS Code command ids to CodeMirror commands**, so existing `keybindings.json` files keep working. Not built. These have no CodeMirror command and need writing:
    - in-place replace;
    - selection anchors;
    - linked editing;
    - move selection to next match;
    - word-highlight navigation.
- **A buffer store that can edit hidden buffers.** Not built; see the buffer section below.
- **Theme conversion** from TextMate scopes to Lezer tags. Built.
- **Language details:**
    - the `${` and `<>` surround pairs;
    - `/**` auto-close;
    - rainbow brackets;
    - indent guides;
    - Format Selection through TypeScript's formatter.

## How the spike built each part

### Buffers: one state per buffer

`CodeMirrorPatchEditor.tsx` keeps one `EditorView` and one `EditorState` per open buffer in a map. On a tab switch it does three things:

1. It stores `{ state: view.state, scroll: view.scrollSnapshot() }` for the outgoing buffer.
2. It calls `view.setState(...)` with the incoming buffer's state.
3. It dispatches the saved scroll effect.

That keeps undo, selection and scroll per buffer with no extra code.

- **Compartments are per state.** The theme and indentation compartments must be reconfigured after every `setState`; otherwise the incoming state keeps whatever look it was created with.
- **External writes** such as a reload from disk replace the whole document in one dispatch. A `writing` flag keeps that change from being echoed back as typing. `createModelValueSync` (shared with Monaco) decides when `value` is newer than the editor's own text.
- **Only the shown buffer is live.** Listeners run only for the state the view is showing, so a buffer the editor is not showing never sees its decorations mapped. Playing-step highlights on a hidden running buffer and mixer edits to a hidden buffer both need a real buffer store. It would hold each buffer's state outside the view and apply transactions to it with `state.update(...)`, and it is the first prep PR to write.

### Reusing Monaco-shaped code: `bufferModel.ts`

`BufferModel` is a small adapter in the shape of a Monaco text model. It provides `getValue`, `getPositionAt`, `getOffsetAt`, `getValueInRange`, `deltaDecorations` and `getDecorationRange`, with ranges mapped through edits as Monaco's tracked ranges are, and ranges that carry a class drawn as marks.

With it, `moduleStateTracking.ts` (playing-step highlights) and the scope anchoring in `patchUi.ts` ran on CodeMirror unchanged. It is a bridge for the spike only. The migration should replace it with an editor-agnostic buffer interface that the mixer, EQ, controls, anchors and performance view all use, landed on master while Monaco is still the editor.

### Scopes: block widgets

`scopeZones.ts` builds each scope as `Decoration.widget({ block: true, side: 1 })` at the end of its call's last line, held in a `StateField` that maps through edits, so a panel stays under its call while lines above it change.

- The widget's `estimatedHeight` is the panel height, so scrolling doesn't jump.
- A `ResizeObserver` resizes the canvas, and only on a change, because assigning a canvas dimension clears it.
- `ignoreEvent() → true` keeps clicks inside the panel from moving the cursor.
- `eq()` compares scope keys, so redrawing the decoration set doesn't recreate canvases.

### Peek: a block widget holding a read-only editor

`peek.ts` puts a block widget under the asking line. The widget holds a location list and a nested read-only `EditorView` that shows the DSL declarations or the patch at the selected location.

- The nested view measures itself, so selecting the first location waits one `requestAnimationFrame` until the panel is in the document.
- Escape closes the peek; double-clicking a location jumps there.
- The nested view uses the same theme extension as the main view.

### Rename box, fix menu and messages: one floating tooltip

`navigation.ts` keeps a single `StateField<Tooltip | null>` (`floatingField`) for whichever of the three is open at the cursor, so they never stack.

- The rename box asks the server for `prepareRename`, offers the name, and applies the `WorkspaceEdit` as one change.
- The fix menu lists the server's quick fixes and our control fix (`computeControlQuickFixes` / `formatControlQuickFix`, shared with Monaco).
- Keys: F12, Alt+F12, Shift+F12, F2, Mod+., Shift+Alt+F. Cmd/Ctrl+click peeks.

### Language features: `languageFeatures.ts`

- **Completion** is an `autocompletion({ override: [source] })`.
    - It filters out the `__` names the patch mapper adds.
    - Items keep the server's order through `boost`.
    - Docs load lazily through `completionItem/resolve` in `info`.
    - It only fires while typing a word or after a `.`, so it doesn't open on every keystroke.
- **Hover** is a `hoverTooltip` rendered with `react-markdown`.
- **Diagnostics** come from a `linter` with a 120 ms delay that pulls `textDocument/diagnostic`. Unused names fade as hints instead of underlining.
- **Signature help** is a `StateField` tooltip driven by a `ViewPlugin`. It re-asks on edits and closes when the cursor leaves the call.
- **Occurrence highlights** are a 150 ms debounced `documentHighlight` request, drawn as marks and cleared on any edit.

### Themes: TextMate scopes to Lezer tags

`theme.ts` holds a table from each Lezer tag to the TextMate scopes that colour it, most specific first. For each tag it picks the theme's longest matching rule, with later rules winning ties, and builds a `HighlightStyle`. Editor chrome (background, gutter, selection, cursor, tooltips) comes from the theme's `colors` through `EditorView.theme`, with a transparent background so video shows through.

The result colours numbers, function calls, properties and labels. Monaco today drops every dotted scope in its own conversion and colours only keywords, strings and comments. So the switch changes how patches look: probably for the better, but it needs your eye. Compare `shots/theme-<name>-monaco.png` with `-codemirror.png`.

### React

- The React Compiler rejects `try`/`finally` inside a component and lint-disable comments. Write helpers like `writeText` outside the component.
- Keep values the editor's callbacks read (`onChange`, the current look) in refs updated by an effect, since CodeMirror extensions keep the closures they were created with.

## Language service

CodeMirror has no TypeScript integration of its own. There are two routes, and the spike proved the second:

1. **Our own `ts.LanguageService` host in a web worker, on the TypeScript 5.9 we already ship.** Today's worker logic (`trackLabelBindings`, `localRefactors`, `dollarQuotes`, `collectSignalSites`) takes `ts` as a parameter, so it ports as it is. That keeps every feature, including extract/inline refactors and the checker-based track diagnostics. `@valtown/codemirror-ts` exists, but we override nearly every method, so our own host is simpler.
2. **TypeScript 7.1's language server in the main process over IPC**, with a content mapper for track labels. It is built and works on both branches.
    - The mapper replaces most of `trackLabelBindings.ts`.
    - 7.1 is a beta.
    - It has no extract or inline refactors and few quick fixes.
    - Output inlay hints need a separate sync-API process: about 45 ms per collection against 1.5 ms in-process.
    - Packaging is unsolved: a 27 MB binary per platform, and the mapper needs a Node runtime.

Either route works with either editor, so the editor and the language service can be decided separately. A CodeMirror migration does not need TypeScript 7.

## Not built in the spike

These are listed roughly by size:

- **The command id table and the keymap.** Every shortcut in `keybindings.json` goes through `editor.trigger` with a VS Code id today. The command palette lists `getSupportedActions()`, and context keys (focus, suggest and find visibility, provider availability) feed our `when` evaluator. All of it needs a CodeMirror-side registry.
- **A buffer store for hidden buffers**, covered above. Mixer edits, control widgets and EQ panels write source into buffers that may not be shown.
- **Inlay hints** (`._cv`, `.mono()`) as inline widgets, including double-click to insert and the fills from the last run.
- **Extract and inline refactors**, with rename after extract. On the TypeScript 7 route they would need writing; on route 1 they port.
- **JSON buffers.** `codemirror-json-schema` covers schema validation, completion and hover. `config.json` allows comments and trailing commas, and whether that package copes with JSONC is unchecked. `keybindings.json` also needs its command-id and `when` completion and warnings.
- **Save-conflict diff**: `@codemirror/merge`, unbuilt.
- **Performance window**: today it uses Monaco's `tokenize`, `TokenTheme` and colour registry. The CodeMirror equivalent is `highlightTree` over a Lezer parse plus the same tag table.
- **Docs and example snippets** (`ColorizedCode.tsx`): `highlightCode`.
- **Video preview panels and EQ panels**: block widgets like scopes. EQ needs mouse and context-menu isolation through `ignoreEvent` and `domEventHandlers`.
- **Code backing** behind text over video: a line or mark decoration.
- **Context menu**: the native Electron menu is built from Monaco actions and their enabled state, so it needs the command registry.
- **Completions**: MIDI device names in `device:`, workspace and `builtin/` paths in strings.
- **Drag a file from the explorer** to insert its quoted path: `domEventHandlers.drop` plus `posAtCoords`.
- **Docs links and experimental tags**: `{@link}` in hovers opening the Docs panel, requoted examples, the `experimental` tag in completion, and F12 on a DSL name opening its docs.
- **Rename validation** of new track names (`trackNameError`), and refusing to rename DSL names.
- **Small language details**: the `${` and `<>` surround pairs, `/**` auto-close, rainbow brackets, indent guides, Format Selection, the `$'…'` quote quick fix.
- **e2e tests.** Eleven e2e files reach into Monaco (its DOM classes such as `.monaco-editor` and `.suggest-widget`, or `window.monaco`). They need an editor-neutral test API first.

## Known limitations of the spike code

- **Rename on a label's mute/solo prefix** offers the whole label instead of refusing.
- **Signature help** scans only the cursor's line for the open parenthesis, so a call split over several lines loses it.
- **Completion triggers** are a heuristic (a word or a `.`). Monaco's trigger characters for strings and paths are not reproduced.
- **The CodeMirror run still loads Monaco**: `bufferModel.ts` imports its `Range`, and the step tracker takes `monaco`. The memory number is therefore an overestimate for CodeMirror.
- **Completion latency** (5.7 ms against 1.3) is the IPC round trip to the server, not CodeMirror. Route 1 would bring it back in-process.

## Migration plan, if it goes ahead

**Prep PRs on master, each running under Monaco:**

1. An editor-agnostic buffer interface and store for the mixer, EQ, controls, anchors and performance view, which can edit buffers that are not shown.
2. A command registry with VS Code ids, replacing the raw `editor.trigger` passthrough, plus the palette and context keys on top of it.
3. An e2e test API that doesn't reach into the editor's DOM or `window.monaco`.
4. A decision on the language service route, landed behind Monaco first if it changes. The TypeScript 7.1 branch already shows Monaco on the server.

**Then an integration branch of stacked PRs:**

1. Core editor, buffers, themes and options.
2. Language features: completion, hover, diagnostics, signature help, highlights, navigation, peek, rename, fixes.
3. Decorations and block widgets: scopes, video previews, EQ, step highlights, code backing, inlay hints.
4. Keymap, palette and context menu.
5. JSON buffers.
6. Diff, snippet colouring and performance view.
7. Remove Monaco.

It merges as one cutover once a parity checklist, built from the Monaco inventory, passes in the app, so master never carries two editors.

## Open questions

- Why move? The answer sets the go/no-go bar.
- Are the CodeMirror theme colours acceptable, or should the conversion copy Monaco's narrower colouring?
- What are the bundle size and cold-start time of each editor? Not measured.
- Which language service route? Route 1 keeps every feature today; route 2 is the TypeScript 7 future but is a beta with gaps.
- Does `codemirror-json-schema` work with JSONC?
