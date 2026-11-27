/**
 * Completes asset paths in string literals: workspace files, and each voice
 * of a `.syx` file, in a literal that starts with `/` (the workspace folder),
 * bundled files in one that starts with `builtin/` or part of it. Also
 * inserts files dropped from the explorer.
 */
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { IRange, editor, languages } from 'monaco-editor';
import type { FileTreeEntry } from '../../../shared/ipcTypes';
import type { AssetKind } from '../../../shared/assetFiles';
import {
    BUILTIN_DRAWINGS,
    BUILTIN_PREFIX,
    BUILTIN_WAVETABLES,
} from '../../../shared/builtinAssets';
import { BUILTIN_DX7_VOICES } from '../../../shared/builtinDx7';
import { type SyxFileVoices, voiceFragments } from '../../../shared/dx7Voice';

/** Drag data type the file explorer sets to a file's `/`-rooted path. */
export const WORKSPACE_PATH_MIME = 'application/x-shoreline-workspace-path';

/** A string literal on one line: its quote and the columns of its contents. */
export interface StringLiteralSpan {
    quote: string;
    /** 1-based column of the first character after the opening quote. */
    contentStart: number;
    /** 1-based column of the closing quote, or one past the line's end. */
    contentEnd: number;
    closed: boolean;
}

/** Characters after which a `/` starts a regular expression rather than dividing. */
const BEFORE_REGEX = new Set('(,=:[!&|?{};+-*%<>~^'.split(''));

/** Keywords after which a `/` starts a regular expression. */
const KEYWORD_BEFORE_REGEX =
    /\b(?:return|typeof|instanceof|in|of|new|delete|void|throw|case|do|else|yield|await)\s*$/;

/**
 * Whether the `/` at `i` starts a regular expression. A regular expression
 * never starts with a space, as formatted code writes it, and postfix `++` or
 * `--` ends a value.
 */
function startsRegex(line: string, i: number): boolean {
    if (line[i + 1] === ' ' || line[i + 1] === undefined) return false;
    const before = line.slice(0, i).trimEnd();
    if (before === '' || KEYWORD_BEFORE_REGEX.test(before)) return true;
    const last = before[before.length - 1];
    if ((last === '+' || last === '-') && before[before.length - 2] === last) {
        return false;
    }
    return BEFORE_REGEX.has(last);
}

/**
 * The string literal whose contents hold `column` (1-based; the column just
 * after the opening quote and the closing quote's own column both count), or
 * null. Scans the one line, skipping comments and regular expressions, so a
 * comment or template literal spanning lines is missed.
 */
export function stringLiteralAt(
    line: string,
    column: number,
): StringLiteralSpan | null {
    let i = 0;
    while (i < line.length) {
        const ch = line[i];
        if (ch === '/' && line[i + 1] === '/') {
            return null;
        }
        // A close with no open on this line ends a comment from lines above.
        if (ch === '*' && line[i + 1] === '/') {
            i += 2;
            continue;
        }
        if (ch === '/' && line[i + 1] === '*') {
            const end = line.indexOf('*/', i + 2);
            if (end === -1) return null;
            i = end + 2;
            continue;
        }
        if (ch === '/' && startsRegex(line, i)) {
            let j = i + 1;
            let inClass = false;
            while (j < line.length && (inClass || line[j] !== '/')) {
                if (line[j] === '[') inClass = true;
                else if (line[j] === ']') inClass = false;
                j += line[j] === '\\' ? 2 : 1;
            }
            i = j + 1;
            continue;
        }
        if (ch !== "'" && ch !== '"' && ch !== '`') {
            i++;
            continue;
        }
        let j = i + 1;
        while (j < line.length && line[j] !== ch) {
            j += line[j] === '\\' ? 2 : 1;
        }
        const closed = j < line.length;
        const span = {
            quote: ch,
            contentStart: i + 2,
            contentEnd: Math.min(j, line.length) + 1,
            closed,
        };
        if (column >= span.contentStart && column <= span.contentEnd) {
            return span;
        }
        i = j + 1;
    }
    return null;
}

const ASSET_DETAILS: Record<AssetKind, string> = {
    wav: 'WAV file',
    image: 'Image',
    video: 'Video',
    drawing: 'SVG drawing',
    syx: 'DX7 voices',
};

/** Every asset in a file tree, with its `/`-rooted workspace path. */
export function assetPaths(
    tree: FileTreeEntry[],
): { path: string; kind: AssetKind }[] {
    return tree.flatMap((entry) => {
        if (entry.type === 'directory') {
            return assetPaths(entry.children ?? []);
        }
        const kind = entry.fileType;
        return kind === undefined || kind === 'patch'
            ? []
            : [{ path: `/${entry.path}`, kind }];
    });
}

/**
 * `path` as a JS string literal, in the first of `preferred`, the other plain
 * quote and a backtick that the path does not hold (`${` counts as holding a
 * backtick). A path holding all three keeps `preferred` and escapes it.
 * Backslashes and line breaks are always escaped.
 */
export function quotePath(path: string, preferred: string): string {
    const holds = (q: string) =>
        path.includes(q) || (q === '`' && path.includes('${'));
    const order = [
        preferred,
        ...["'", '"', '`'].filter((q) => q !== preferred),
    ];
    const quote = order.find((q) => !holds(q)) ?? preferred;
    let body = path
        .replace(/\\/g, '\\\\')
        .replace(/\n/g, '\\n')
        .replace(/\r/g, '\\r')
        .replaceAll(quote, `\\${quote}`);
    if (quote === '`') {
        body = body.replace(/\$\{/g, '\\${');
    }
    return `${quote}${body}${quote}`;
}

/** A path offered for completion and the words shown beside it. */
export interface PathSuggestion {
    path: string;
    detail: string;
}

/** A voice's completion: its file alone when the file holds only it. */
function voiceSuggestions(
    path: string,
    names: readonly string[],
): PathSuggestion[] {
    if (names.length === 1) {
        return [{ path, detail: `DX7 voice ${names[0]}` }];
    }
    const fragments = voiceFragments(names);
    return [
        { path, detail: `${ASSET_DETAILS.syx}: the first, ${names[0]}` },
        ...fragments.map((fragment, i) => ({
            path: `${path}#${fragment}`,
            detail: `DX7 voice ${i + 1} of ${names.length}: ${names[i]}`,
        })),
    ];
}

/**
 * The completions for a workspace's `assets`: each asset, and every voice of
 * each `.syx` file whose names `voices` holds.
 */
export function workspaceSuggestions(
    assets: readonly { path: string; kind: AssetKind }[],
    voices: readonly SyxFileVoices[],
): PathSuggestion[] {
    const names = new Map(voices.map((v) => [v.path, v.names]));
    return assets.flatMap(({ path, kind }) => {
        const fileNames = kind === 'syx' ? names.get(path) : undefined;
        return fileNames && fileNames.length > 0
            ? voiceSuggestions(path, fileNames)
            : [{ path, detail: ASSET_DETAILS[kind] }];
    });
}

const BUILTIN_SUGGESTIONS: PathSuggestion[] = [
    ...BUILTIN_WAVETABLES.map(({ path }) => ({
        path,
        detail: ASSET_DETAILS.wav,
    })),
    ...BUILTIN_DRAWINGS.map(({ path }) => ({
        path,
        detail: ASSET_DETAILS.drawing,
    })),
    ...BUILTIN_DX7_VOICES.flatMap(({ path, name }) =>
        voiceSuggestions(path, [name]),
    ),
];

/**
 * Each suggestion's literal in each quote, computed once per list: a list
 * holds thousands of entries and is offered on every request.
 */
const quotedCache = new WeakMap<
    readonly PathSuggestion[],
    Map<string, string[]>
>();

function quoted(list: readonly PathSuggestion[], quote: string): string[] {
    let byQuote = quotedCache.get(list);
    if (!byQuote) {
        byQuote = new Map();
        quotedCache.set(list, byQuote);
    }
    let literals = byQuote.get(quote);
    if (!literals) {
        literals = list.map(({ path }) => quotePath(path, quote));
        byQuote.set(quote, literals);
    }
    return literals;
}

/**
 * Registers completion of asset paths inside string literals: workspace files
 * and the voices of `.syx` files when the literal starts with `/`, bundled
 * files when it starts with `builtin/` or part of it. `workspace` gives the
 * workspace's suggestions; it should answer from a cache, since it is asked
 * on every request.
 */
export function registerWorkspacePathCompletionProvider(
    monaco: Monaco,
    workspace: () => Promise<readonly PathSuggestion[]>,
): { dispose: () => void } {
    const provider: languages.CompletionItemProvider = {
        triggerCharacters: ['/', '#'],
        async provideCompletionItems(model, position) {
            const line = model.getLineContent(position.lineNumber);
            const span = stringLiteralAt(line, position.column);
            if (!span) return undefined;
            const typed = line.slice(
                span.contentStart - 1,
                position.column - 1,
            );
            const inWorkspace = typed.startsWith('/');
            const builtin =
                typed !== '' &&
                (BUILTIN_PREFIX.startsWith(typed) ||
                    typed.startsWith(BUILTIN_PREFIX));
            if (!inWorkspace && !builtin) return undefined;
            const lists = [
                inWorkspace ? await workspace() : [],
                builtin ? BUILTIN_SUGGESTIONS : [],
            ];
            // The whole literal, quotes included, is replaced: the path may
            // need the other quote, and accepting never leaves the rest of an
            // earlier path behind. In an unclosed literal the rest of the line
            // is other code.
            const range: IRange = {
                startLineNumber: position.lineNumber,
                endLineNumber: position.lineNumber,
                startColumn: span.contentStart - 1,
                endColumn: span.closed ? span.contentEnd + 1 : position.column,
            };
            const kind = monaco.languages.CompletionItemKind.File;
            const suggestions: languages.CompletionItem[] = [];
            for (const list of lists) {
                const literals = quoted(list, span.quote);
                for (let i = 0; i < list.length; i++) {
                    const { path, detail } = list[i];
                    suggestions.push({
                        label: path,
                        kind,
                        detail,
                        insertText: literals[i],
                        filterText: `${span.quote}${path}`,
                        range,
                    });
                }
            }
            // Complete: Monaco filters this list as typing goes on and asks
            // again only at a trigger character.
            return { suggestions, incomplete: false };
        },
    };
    return monaco.languages.registerCompletionItemProvider(
        'javascript',
        provider,
    );
}

/**
 * Lets a file dragged from the explorer drop into the editor: onto a string
 * literal it replaces the literal with the path, elsewhere it inserts the path
 * as a new literal. Either way `quotePath` quotes it, preferring `quote()`.
 */
export function installWorkspacePathDrop(
    editorInstance: editor.IStandaloneCodeEditor,
    quote: () => string,
): { dispose: () => void } {
    // The container lives as long as the editor; the node `getDomNode()`
    // returns is rebuilt on every model change.
    const node = editorInstance.getContainerDomNode();
    const carriesPath = (e: DragEvent) =>
        e.dataTransfer?.types.includes(WORKSPACE_PATH_MIME) ?? false;

    // Capture phase, so Monaco's own drop handling on the same container
    // never sees these drags.
    const onDragOver = (e: DragEvent) => {
        if (!carriesPath(e)) return;
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer!.dropEffect = 'copy';
    };
    const onDrop = (e: DragEvent) => {
        if (!carriesPath(e)) return;
        e.preventDefault();
        e.stopPropagation();
        const path = e.dataTransfer!.getData(WORKSPACE_PATH_MIME);
        const model = editorInstance.getModel();
        const position = editorInstance.getTargetAtClientPoint(
            e.clientX,
            e.clientY,
        )?.position;
        if (!path || !model || !position) return;

        const line = model.getLineContent(position.lineNumber);
        const found = stringLiteralAt(line, position.column);
        const span = found?.closed ? found : null;
        const range = span
            ? {
                  startLineNumber: position.lineNumber,
                  endLineNumber: position.lineNumber,
                  startColumn: span.contentStart - 1,
                  endColumn: span.contentEnd + 1,
              }
            : {
                  startLineNumber: position.lineNumber,
                  endLineNumber: position.lineNumber,
                  startColumn: position.column,
                  endColumn: position.column,
              };
        const text = quotePath(path, quote());
        editorInstance.focus();
        editorInstance.executeEdits('workspace-path-drop', [
            { range, text, forceMoveMarkers: true },
        ]);
        editorInstance.setPosition({
            lineNumber: position.lineNumber,
            column: range.startColumn + text.length,
        });
    };
    node.addEventListener('dragover', onDragOver, true);
    node.addEventListener('drop', onDrop, true);
    return {
        dispose: () => {
            node.removeEventListener('dragover', onDragOver, true);
            node.removeEventListener('drop', onDrop, true);
        },
    };
}
