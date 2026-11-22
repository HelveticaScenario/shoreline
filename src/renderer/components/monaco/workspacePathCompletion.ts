/**
 * Completes asset paths in string literals: workspace files in a literal that
 * starts with `/` (the workspace folder), bundled files in one that starts
 * with `builtin/` or part of it. Also inserts files dropped from the explorer.
 */
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { IRange, editor, languages } from 'monaco-editor';
import type { FileTreeEntry } from '../../../shared/ipcTypes';
import type { AssetKind } from '../../../shared/assetFiles';
import {
    BUILTIN_PREFIX,
    BUILTIN_WAVETABLES,
} from '../../../shared/builtinAssets';

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
interface PathSuggestion {
    path: string;
    detail: string;
}

const BUILTIN_SUGGESTIONS: PathSuggestion[] = BUILTIN_WAVETABLES.map(
    ({ path }) => ({ path, detail: ASSET_DETAILS.wav }),
);

/** The workspace assets `listFiles` lists, or none when it fails. */
async function workspaceSuggestions(
    listFiles: () => Promise<FileTreeEntry[]>,
): Promise<PathSuggestion[]> {
    try {
        return assetPaths(await listFiles()).map(({ path, kind }) => ({
            path,
            detail: ASSET_DETAILS[kind],
        }));
    } catch (error) {
        console.error('[path completion] failed to list files', error);
        return [];
    }
}

/**
 * Registers completion of asset paths inside string literals: workspace files
 * when the literal starts with `/`, bundled files when it starts with
 * `builtin/` or part of it. `listFiles` is read
 * on every request, so files added since the editor opened are offered.
 */
export function registerWorkspacePathCompletionProvider(
    monaco: Monaco,
    listFiles: () => Promise<FileTreeEntry[]>,
): { dispose: () => void } {
    const provider: languages.CompletionItemProvider = {
        triggerCharacters: ['/'],
        async provideCompletionItems(model, position) {
            const line = model.getLineContent(position.lineNumber);
            const span = stringLiteralAt(line, position.column);
            if (!span) return undefined;
            const typed = line.slice(
                span.contentStart - 1,
                position.column - 1,
            );
            const workspace = typed.startsWith('/');
            const builtin =
                typed !== '' &&
                (BUILTIN_PREFIX.startsWith(typed) ||
                    typed.startsWith(BUILTIN_PREFIX));
            if (!workspace && !builtin) return undefined;
            const offered = [
                ...(workspace ? await workspaceSuggestions(listFiles) : []),
                ...(builtin ? BUILTIN_SUGGESTIONS : []),
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
            return {
                suggestions: offered.map(({ path, detail }) => ({
                    label: path,
                    kind: monaco.languages.CompletionItemKind.File,
                    detail,
                    insertText: quotePath(path, span.quote),
                    filterText: `${span.quote}${path}`,
                    range,
                })),
            };
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
