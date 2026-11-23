import { describe, expect, test } from 'vitest';
import type { EditorBuffer } from '../../types/editor';
import {
    closeBufferIn,
    newFileBuffer,
    nextUntitledId,
    resolveFileTarget,
    withOpenedBuffer,
    withUntitledSavedAs,
} from '../bufferList';
import { getBufferId, renamedFilePath } from '../buffers';

const file = (
    filePath: string,
    overrides: Partial<EditorBuffer> = {},
): EditorBuffer =>
    ({
        content: 'saved',
        dirty: false,
        filePath,
        id: filePath,
        kind: 'file',
        savedContent: 'saved',
        ...overrides,
    }) as EditorBuffer;

const untitled = (id: string, content = 'draft'): EditorBuffer => ({
    content,
    dirty: true,
    id,
    kind: 'untitled',
});

const ids = (buffers: EditorBuffer[]) => buffers.map(getBufferId);

describe('withOpenedBuffer', () => {
    test('a preview takes the place of a clean preview', () => {
        const opened = withOpenedBuffer(
            [file('/w/a.sl'), file('/w/b.sl', { isPreview: true })],
            newFileBuffer('/w/c.sl', 'c', true),
        );
        expect(ids(opened)).toEqual(['/w/a.sl', '/w/c.sl']);
    });

    test('a preview with unsaved edits stays open', () => {
        const opened = withOpenedBuffer(
            [file('/w/b.sl', { dirty: true, isPreview: true })],
            newFileBuffer('/w/c.sl', 'c', true),
        );
        expect(ids(opened)).toEqual(['/w/b.sl', '/w/c.sl']);
    });

    test('a kept buffer leaves the preview open', () => {
        const opened = withOpenedBuffer(
            [file('/w/b.sl', { isPreview: true })],
            newFileBuffer('/w/c.sl', 'c', false),
        );
        expect(ids(opened)).toEqual(['/w/b.sl', '/w/c.sl']);
    });
});

describe('nextUntitledId', () => {
    test('takes the lowest number no buffer of any kind holds', () => {
        expect(nextUntitledId([])).toBe('untitled-1');
        expect(
            nextUntitledId([
                file('/w/a.sl', { id: 'untitled-1' }),
                untitled('untitled-3'),
            ]),
        ).toBe('untitled-2');
    });
});

describe('withUntitledSavedAs', () => {
    test('the buffer becomes a file buffer keeping its id and later edits', () => {
        const [saved] = withUntitledSavedAs(
            [untitled('untitled-1', 'typed since')],
            'untitled-1',
            '/w/a.sl',
            'written',
        );
        expect(saved).toEqual({
            content: 'typed since',
            dirty: true,
            filePath: '/w/a.sl',
            id: 'untitled-1',
            kind: 'file',
            savedContent: 'written',
        });
    });

    test('saving over an open file folds into that buffer', () => {
        const saved = withUntitledSavedAs(
            [
                file('/w/a.sl', { id: 'a', isPreview: true }),
                untitled('untitled-1', 'written'),
            ],
            'untitled-1',
            '/w/a.sl',
            'written',
        );
        expect(saved).toEqual([
            file('/w/a.sl', {
                content: 'written',
                id: 'a',
                isPreview: false,
                savedContent: 'written',
            } as Partial<EditorBuffer>),
        ]);
    });
});

describe('closeBufferIn', () => {
    const buffers = [file('/w/a.sl'), file('/w/b.sl'), file('/w/c.sl')];

    test('the buffer after the closed one is next', () => {
        const closed = closeBufferIn(buffers, '/w/b.sl');
        expect(ids(closed!.remaining)).toEqual(['/w/a.sl', '/w/c.sl']);
        expect(closed!.nextActiveId).toBe('/w/c.sl');
    });

    test('closing the tail selects the new tail', () => {
        expect(closeBufferIn(buffers, '/w/c.sl')!.nextActiveId).toBe('/w/b.sl');
    });

    test('closing the only buffer leaves none active', () => {
        expect(closeBufferIn([file('/w/a.sl')], '/w/a.sl')).toEqual({
            nextActiveId: undefined,
            remaining: [],
        });
    });

    test('an unknown id closes nothing', () => {
        expect(closeBufferIn(buffers, '/w/x.sl')).toBeUndefined();
    });
});

describe('resolveFileTarget', () => {
    const buffers = [file('/w/a.sl'), untitled('untitled-1')];

    test('a buffer id or a workspace-relative path names an open file', () => {
        const open = { filePath: '/w/a.sl', isOpen: true };
        expect(resolveFileTarget(buffers, undefined, '/w', '/w/a.sl')).toEqual(
            open,
        );
        expect(resolveFileTarget(buffers, undefined, '/w', 'a.sl')).toEqual(
            open,
        );
    });

    test('a path with no buffer resolves against the workspace', () => {
        expect(resolveFileTarget(buffers, '/w/a.sl', '/w', 'b.sl')).toEqual({
            filePath: '/w/b.sl',
            isOpen: false,
        });
    });

    test('no target falls back to the active file buffer', () => {
        expect(resolveFileTarget(buffers, '/w/a.sl', '/w', undefined)).toEqual({
            filePath: '/w/a.sl',
            isOpen: true,
        });
        expect(
            resolveFileTarget(buffers, 'untitled-1', '/w', undefined),
        ).toBeUndefined();
    });
});

describe('renamedFilePath', () => {
    test('the file stays in its folder under the new name', () => {
        expect(renamedFilePath('/w/songs/old.mjs', 'new')).toBe(
            '/w/songs/new.mjs',
        );
        expect(renamedFilePath('C:\\w\\old.sl', 'new')).toBe('C:\\w\\new.sl');
    });

    test('an empty or unchanged name renames nothing', () => {
        expect(renamedFilePath('/w/old.sl', '')).toBeUndefined();
        expect(renamedFilePath('/w/old.sl', 'old.sl')).toBeUndefined();
        expect(renamedFilePath('/w/old.sl', 'old')).toBeUndefined();
    });
});
