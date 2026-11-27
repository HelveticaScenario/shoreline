import type { Text } from '@codemirror/state';
import type { LanguageServerInfo } from '../../../shared/ipcTypes';
import electronAPI from '../../electronAPI';

export interface LspPosition {
    line: number;
    character: number;
}

export interface LspRange {
    start: LspPosition;
    end: LspPosition;
}

export interface LspLocation {
    uri: string;
    range: LspRange;
}

export interface LspTextEdit {
    range: LspRange;
    newText: string;
}

export interface LspWorkspaceEdit {
    changes?: Record<string, LspTextEdit[]>;
    documentChanges?: { textDocument: { uri: string }; edits: LspTextEdit[] }[];
}

/** How long each kind of request took, for the editor's own profiling. */
export interface RequestTiming {
    method: string;
    ms: number;
}

/**
 * The renderer's end of the language server: document sync and requests for
 * the patch buffers, each a virtual `.sl` document.
 */
export class LanguageClient {
    readonly timings: RequestTiming[] = [];
    private readonly versions = new Map<string, number>();

    constructor(readonly info: LanguageServerInfo) {}

    uriFor(bufferId: string): string {
        return `${this.info.buffersUri}/${encodeURIComponent(bufferId).replace(/%/g, '_')}.sl`;
    }

    open(uri: string, text: string): void {
        if (this.versions.has(uri)) {
            this.change(uri, text);
            return;
        }
        this.versions.set(uri, 1);
        void electronAPI.languageServer.notify('textDocument/didOpen', {
            textDocument: { languageId: 'javascript', text, uri, version: 1 },
        });
    }

    change(uri: string, text: string): void {
        const version = (this.versions.get(uri) ?? 0) + 1;
        this.versions.set(uri, version);
        void electronAPI.languageServer.notify('textDocument/didChange', {
            contentChanges: [{ text }],
            textDocument: { uri, version },
        });
    }

    close(uri: string): void {
        if (!this.versions.delete(uri)) {
            return;
        }
        void electronAPI.languageServer.notify('textDocument/didClose', {
            textDocument: { uri },
        });
    }

    async request<T>(method: string, params: unknown): Promise<T | null> {
        const started = performance.now();
        try {
            return (await electronAPI.languageServer.request(
                method,
                params,
            )) as T | null;
        } catch {
            // A request the server cancels or cannot answer shows nothing.
            return null;
        } finally {
            this.timings.push({ method, ms: performance.now() - started });
            if (this.timings.length > 2000) {
                this.timings.splice(0, 1000);
            }
        }
    }
}

export function toLspPosition(doc: Text, offset: number): LspPosition {
    const line = doc.lineAt(offset);
    return { character: offset - line.from, line: line.number - 1 };
}

export function fromLspPosition(doc: Text, position: LspPosition): number {
    if (position.line >= doc.lines) {
        return doc.length;
    }
    const line = doc.line(position.line + 1);
    return Math.min(line.to, line.from + position.character);
}

export function fromLspRange(
    doc: Text,
    range: LspRange,
): { from: number; to: number } {
    return {
        from: fromLspPosition(doc, range.start),
        to: fromLspPosition(doc, range.end),
    };
}

/** The edits a workspace edit makes to `uri`, as editor changes. */
export function changesFor(
    doc: Text,
    edit: LspWorkspaceEdit | null,
    uri: string,
): { from: number; to: number; insert: string }[] {
    const edits = [
        ...(edit?.changes?.[uri] ?? []),
        ...(edit?.documentChanges ?? [])
            .filter((change) => change.textDocument.uri === uri)
            .flatMap((change) => change.edits),
    ];
    return edits.map((e) => ({
        ...fromLspRange(doc, e.range),
        insert: e.newText,
    }));
}
