import { describe, expect, test } from 'vitest';
import type { EditorBuffer } from '../../types/editor';
import {
    newFileBuffer,
    nextUntitledId,
    resolveFileTarget,
    withOpenedBuffer,
    withUntitledSavedAs,
} from '../bufferList';
import { getBufferId } from '../buffers';

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

describe('resolveFileTarget', () => {
    const buffers = [file('/w/a.sl'), untitled('untitled-1')];

    test('a buffer id or a workspace-relative path names an open file', () => {
        expect(resolveFileTarget(buffers, undefined, '/w', '/w/a.sl')).toBe(
            '/w/a.sl',
        );
        expect(resolveFileTarget(buffers, undefined, '/w', 'a.sl')).toBe(
            '/w/a.sl',
        );
    });

    test('a path with no buffer resolves against the workspace', () => {
        expect(resolveFileTarget(buffers, '/w/a.sl', '/w', 'b.sl')).toBe(
            '/w/b.sl',
        );
    });

    test('no target falls back to the active file buffer', () => {
        expect(resolveFileTarget(buffers, '/w/a.sl', '/w', undefined)).toBe(
            '/w/a.sl',
        );
        expect(
            resolveFileTarget(buffers, 'untitled-1', '/w', undefined),
        ).toBeUndefined();
    });
});
