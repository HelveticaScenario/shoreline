/**
 * The inlay-hint provider over a stand-in Monaco: a model holding plain text,
 * decorations that keep the offsets they were given, and a worker handing
 * back fixed sites.
 */

import { beforeEach, describe, expect, test, vi } from 'vitest';
import type { languages } from 'monaco-editor';
import type { ModuleChannelsRecord } from '../../../../shared/dsl/channels';
import type { SignalSite } from '../../../../shared/dsl/signalSites';
import type { Monaco } from '../../../hooks/useCustomMonaco';
import {
    applyRuntimeResolutions,
    registerOutputResolutionHints,
} from '../outputResolutionHints';

vi.mock('../../../electronAPI', () => ({
    default: { getSignalSiteTables: async () => ({}) },
}));

interface Position {
    lineNumber: number;
    column: number;
}

class Range {
    constructor(
        readonly startLineNumber: number,
        readonly startColumn: number,
        readonly endLineNumber: number,
        readonly endColumn: number,
    ) {}
    static fromPositions(start: Position): Range {
        return new Range(
            start.lineNumber,
            start.column,
            start.lineNumber,
            start.column,
        );
    }
    getStartPosition(): Position {
        return { column: this.startColumn, lineNumber: this.startLineNumber };
    }
    getEndPosition(): Position {
        return { column: this.endColumn, lineNumber: this.endLineNumber };
    }
    containsPosition(): boolean {
        return true;
    }
}

/** A model whose decorations stay at the offsets they were set at. */
class FakeModel {
    readonly uri = { toString: () => 'file:///patch.js' };
    private decorations = new Map<string, [number, number]>();
    private nextId = 0;
    constructor(public text: string) {}
    getValue(): string {
        return this.text;
    }
    isDisposed(): boolean {
        return false;
    }
    getPositionAt(offset: number): Position {
        const before = this.text.slice(0, offset).split('\n');
        return {
            column: before[before.length - 1].length + 1,
            lineNumber: before.length,
        };
    }
    getOffsetAt(position: Position): number {
        const lines = this.text.split('\n');
        let offset = 0;
        for (let i = 0; i < position.lineNumber - 1; i++) {
            offset += lines[i].length + 1;
        }
        return offset + position.column - 1;
    }
    getValueInRange(range: Range): string {
        return this.text.slice(
            this.getOffsetAt(range.getStartPosition()),
            this.getOffsetAt(range.getEndPosition()),
        );
    }
    deltaDecorations(old: string[], added: { range: Range }[]): string[] {
        for (const id of old) {
            this.decorations.delete(id);
        }
        return added.map(({ range }) => {
            const id = String(this.nextId++);
            this.decorations.set(id, [
                this.getOffsetAt(range.getStartPosition()),
                this.getOffsetAt(range.getEndPosition()),
            ]);
            return id;
        });
    }
    getDecorationRange(id: string): Range | null {
        const span = this.decorations.get(id);
        if (!span) {
            return null;
        }
        const start = this.getPositionAt(span[0]);
        const end = this.getPositionAt(span[1]);
        return new Range(
            start.lineNumber,
            start.column,
            end.lineNumber,
            end.column,
        );
    }
}

const SOURCE = `out$0m: $sampler('/mono.wav', 5)`;
const CALL_START = SOURCE.indexOf('$sampler');

/** The mono track's receiver, which folds while its file may be stereo. */
const SITE: SignalSite = {
    column: 2,
    end: SOURCE.length,
    fold: true,
    foldCalls: [
        {
            column: CALL_START + 1,
            end: SOURCE.length,
            line: 1,
            module: '$sampler',
            start: CALL_START,
        },
    ],
    insertable: true,
    line: 1,
    slot: 'receiver',
    start: CALL_START,
};

/** What the run records for the call, its first-line column offset included. */
const record = (channels: 'mono' | 'stereo'): ModuleChannelsRecord => ({
    channels,
    column: CALL_START + 1 + 4,
    line: 1,
    module: '$sampler',
});

let provider: languages.InlayHintsProvider;
let sites: SignalSite[];

beforeEach(() => {
    sites = [SITE];
    const monaco = {
        Range,
        editor: { TrackedRangeStickiness: { NeverGrowsWhenTypingAtEdges: 1 } },
        languages: {
            registerInlayHintsProvider: (
                _language: string,
                registered: languages.InlayHintsProvider,
            ) => {
                provider = registered;
                return { dispose: () => {} };
            },
        },
        typescript: {
            getJavaScriptWorker: async () => async () => ({
                getSignalSites: async () => sites,
            }),
        },
    };
    registerOutputResolutionHints(monaco as unknown as Monaco);
});

async function labels(model: FakeModel): Promise<string[]> {
    const result = await provider.provideInlayHints(
        model as never,
        new Range(1, 1, 1, 1) as never,
        { isCancellationRequested: false } as never,
    );
    return (result?.hints ?? []).map((h) => h.label as string);
}

describe('fold hints through files', () => {
    test('a file call may be stereo before a run', async () => {
        expect(await labels(new FakeModel(SOURCE))).toEqual(['.mono()']);
    });

    test('a run that finds the file mono drops the fold', async () => {
        const model = new FakeModel(SOURCE);
        await applyRuntimeResolutions(
            model as never,
            SOURCE,
            [],
            [record('mono')],
        );
        expect(await labels(model)).toEqual([]);
    });

    test('a run that finds the file stereo keeps it', async () => {
        const model = new FakeModel(SOURCE);
        await applyRuntimeResolutions(
            model as never,
            SOURCE,
            [],
            [record('stereo')],
        );
        expect(await labels(model)).toEqual(['.mono()']);
    });

    test('the fold returns once the call reads differently', async () => {
        const model = new FakeModel(SOURCE);
        await applyRuntimeResolutions(
            model as never,
            SOURCE,
            [],
            [record('mono')],
        );
        model.text = SOURCE.replace('mono.wav', 'pair.wav');
        expect(await labels(model)).toEqual(['.mono()']);
    });

    test('a run of other text changes nothing', async () => {
        const model = new FakeModel(SOURCE);
        await applyRuntimeResolutions(
            model as never,
            `${SOURCE} `,
            [],
            [record('mono')],
        );
        expect(await labels(model)).toEqual(['.mono()']);
    });
});
