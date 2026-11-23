import { describe, expect, test } from 'vitest';
import type { EditorBuffer, FileBuffer } from '../../types/editor';
import {
    bufferReducer,
    initialBufferState,
    type BufferAction,
    type BufferState,
} from '../bufferState';
import { getBufferId } from '../buffers';

const file = (
    filePath: string,
    overrides: Partial<FileBuffer> = {},
): FileBuffer => ({
    content: 'saved',
    dirty: false,
    filePath,
    id: filePath,
    kind: 'file',
    savedContent: 'saved',
    ...overrides,
});

const untitled = (id: string, content = 'draft'): EditorBuffer => ({
    content,
    dirty: true,
    id,
    kind: 'untitled',
});

const A = '/w/a.sl';
const B = '/w/b.sl';
const C = '/w/c.sl';

const state = (
    buffers: EditorBuffer[],
    activeBufferId: string | undefined,
): BufferState => ({ activeBufferId, buffers });

const run = (from: BufferState, ...actions: BufferAction[]) =>
    actions.reduce(bufferReducer, from);

const ids = (s: BufferState) => s.buffers.map(getBufferId);

describe('initialBufferState', () => {
    test('the first restored buffer is active', () => {
        expect(initialBufferState([file(A), file(B)]).activeBufferId).toBe(A);
        expect(initialBufferState([]).activeBufferId).toBeUndefined();
    });
});

describe('openFile', () => {
    test('the opened file is added and shown', () => {
        const next = run(state([file(A)], A), {
            buffer: file(B),
            keep: false,
            type: 'openFile',
        });
        expect(ids(next)).toEqual([A, B]);
        expect(next.activeBufferId).toBe(B);
    });

    test('a path is open in one buffer however often it is opened', () => {
        const edited = file(A, { content: 'mine', dirty: true });
        const next = run(
            state([edited, file(B)], B),
            {
                buffer: file(A, { id: 'second' }),
                keep: false,
                type: 'openFile',
            },
            { buffer: file(A, { id: 'third' }), keep: false, type: 'openFile' },
        );
        expect(next.buffers).toEqual([edited, file(B)]);
        expect(next.activeBufferId).toBe(A);
    });

    test('opening a previewed path to keep ends its preview', () => {
        const next = run(
            state([], undefined),
            {
                buffer: file(A, { isPreview: true }),
                keep: false,
                type: 'openFile',
            },
            {
                buffer: file(A, { id: 'second', isPreview: false }),
                keep: true,
                type: 'openFile',
            },
        );
        expect(next.buffers).toEqual([file(A, { isPreview: false })]);
    });
});

describe('showFile', () => {
    test('shows the open buffer, leaving its preview unless kept', () => {
        const from = state([file(A, { isPreview: true }), file(B)], B);
        const shown = run(from, { filePath: A, keep: false, type: 'showFile' });
        expect(shown.activeBufferId).toBe(A);
        expect(shown.buffers[0].isPreview).toBe(true);
        const kept = run(from, { filePath: A, keep: true, type: 'showFile' });
        expect(kept.buffers[0].isPreview).toBe(false);
    });

    test('a path with no buffer changes nothing', () => {
        const from = state([file(A)], A);
        expect(run(from, { filePath: B, keep: true, type: 'showFile' })).toBe(
            from,
        );
    });
});

describe('createUntitled', () => {
    test('the new buffer takes the lowest free id and is shown', () => {
        const next = run(
            state([file(A, { id: 'untitled-1' })], A),
            { content: 'patch', type: 'createUntitled' },
            { content: 'patch', type: 'createUntitled' },
        );
        expect(ids(next)).toEqual([A, 'untitled-2', 'untitled-3']);
        expect(next.activeBufferId).toBe('untitled-3');
        expect(next.buffers[1]).toEqual({
            content: 'patch',
            dirty: false,
            id: 'untitled-2',
            kind: 'untitled',
        });
    });
});

describe('setActive', () => {
    test('only an open buffer can become active', () => {
        const from = state([file(A), file(B)], A);
        expect(
            run(from, { bufferId: B, type: 'setActive' }).activeBufferId,
        ).toBe(B);
        expect(run(from, { bufferId: C, type: 'setActive' })).toBe(from);
    });
});

describe('edit', () => {
    test('the buffer takes the content and stops being a preview', () => {
        const next = run(state([file(A, { isPreview: true }), file(B)], A), {
            bufferId: A,
            content: 'typed',
            type: 'edit',
        });
        expect(next.buffers).toEqual([
            file(A, { content: 'typed', dirty: true, isPreview: false }),
            file(B),
        ]);
    });
});

describe('close', () => {
    const open = state([file(A), file(B), file(C)], B);

    test('closing the active buffer shows the one after it', () => {
        const next = run(open, { bufferId: B, type: 'close' });
        expect(ids(next)).toEqual([A, C]);
        expect(next.activeBufferId).toBe(C);
    });

    test('closing the active tail shows the new tail', () => {
        const next = run(
            { ...open, activeBufferId: C },
            { bufferId: C, type: 'close' },
        );
        expect(next.activeBufferId).toBe(B);
    });

    test('closing another buffer keeps the active one', () => {
        const next = run(open, { bufferId: A, type: 'close' });
        expect(ids(next)).toEqual([B, C]);
        expect(next.activeBufferId).toBe(B);
    });

    test('closing the only buffer leaves none active', () => {
        expect(
            run(state([file(A)], A), { bufferId: A, type: 'close' }),
        ).toEqual(state([], undefined));
    });

    test('an unknown id closes nothing', () => {
        expect(run(open, { bufferId: '/w/x.sl', type: 'close' })).toBe(open);
    });

    test('a buffer saved from untitled closes under its path while active', () => {
        const next = run(
            state([file(A), untitled('untitled-1')], 'untitled-1'),
            {
                filePath: B,
                savedContent: 'draft',
                type: 'saveUntitledAs',
                untitledId: 'untitled-1',
            },
            { bufferId: B, type: 'close' },
        );
        expect(next).toEqual(state([file(A)], A));
    });
});

describe('deleteFile', () => {
    const open = state([file(A), file(B), file(C)], B);

    test('deleting the active file shows the same buffer closing it would', () => {
        expect(run(open, { filePath: B, type: 'deleteFile' })).toEqual(
            run(open, { bufferId: B, type: 'close' }),
        );
        const tail = { ...open, activeBufferId: C };
        expect(run(tail, { filePath: C, type: 'deleteFile' })).toEqual(
            run(tail, { bufferId: C, type: 'close' }),
        );
    });

    test('the active id never names the deleted file, whatever preceded', () => {
        const next = run(
            open,
            { bufferId: A, content: 'typed', type: 'edit' },
            { bufferId: B, type: 'setActive' },
            { filePath: B, type: 'deleteFile' },
        );
        expect(ids(next)).toEqual([A, C]);
        expect(next.activeBufferId).toBe(C);
    });

    test('deleting another file keeps the active buffer', () => {
        const next = run(open, { filePath: C, type: 'deleteFile' });
        expect(ids(next)).toEqual([A, B]);
        expect(next.activeBufferId).toBe(B);
    });

    test('a file with no buffer changes nothing', () => {
        expect(run(open, { filePath: '/w/x.sl', type: 'deleteFile' })).toBe(
            open,
        );
    });
});

describe('renameFile', () => {
    test('the buffer and the active id follow the file to its new path', () => {
        const next = run(state([file(A), file(B)], A), {
            newPath: C,
            oldPath: A,
            type: 'renameFile',
        });
        expect(ids(next)).toEqual([C, B]);
        expect(next.activeBufferId).toBe(C);
        expect(next.buffers[0].id).toBe(A);
    });

    test('renaming an inactive or unopened file keeps the active buffer', () => {
        const from = state([file(A), file(B)], B);
        expect(
            run(from, { newPath: C, oldPath: A, type: 'renameFile' })
                .activeBufferId,
        ).toBe(B);
        expect(
            run(from, { newPath: C, oldPath: '/w/x.sl', type: 'renameFile' }),
        ).toBe(from);
    });
});

describe('saveUntitledAs', () => {
    const save: BufferAction = {
        filePath: A,
        savedContent: 'draft',
        type: 'saveUntitledAs',
        untitledId: 'untitled-1',
    };

    test('an active untitled buffer stays active under its file path', () => {
        const next = run(state([untitled('untitled-1')], 'untitled-1'), save);
        expect(ids(next)).toEqual([A]);
        expect(next.activeBufferId).toBe(A);
    });

    test('an inactive one leaves the active buffer alone', () => {
        const next = run(state([untitled('untitled-1'), file(B)], B), save);
        expect(next.activeBufferId).toBe(B);
    });

    test('saving over an open file leaves one buffer, active if either was', () => {
        const next = run(
            state([file(A), untitled('untitled-1')], 'untitled-1'),
            save,
        );
        expect(ids(next)).toEqual([A]);
        expect(next.activeBufferId).toBe(A);
    });
});

describe('disk content', () => {
    test('a save marks the buffer clean only if it still holds what was written', () => {
        const from = state(
            [file(A, { content: 'newer', dirty: true, savedContent: 'old' })],
            A,
        );
        const next = run(from, {
            filePath: A,
            savedContent: 'written',
            type: 'fileSaved',
        });
        expect(next.buffers[0]).toEqual(
            file(A, { content: 'newer', dirty: true, savedContent: 'written' }),
        );
    });

    test('a clean buffer takes a disk change, a dirty one keeps its edits', () => {
        const dirty = file(B, { content: 'mine', dirty: true });
        const next = run(
            state([file(A), dirty], A),
            { content: 'theirs', filePath: A, type: 'diskChanged' },
            { content: 'theirs', filePath: B, type: 'diskChanged' },
        );
        expect(next.buffers).toEqual([
            file(A, { content: 'theirs', savedContent: 'theirs' }),
            dirty,
        ]);
    });

    test('a restored buffer without a baseline adopts the disk content as one', () => {
        const restored = file(A, {
            content: 'mine',
            dirty: true,
            savedContent: null,
        });
        const next = run(state([restored], A), {
            baseline: null,
            diskContent: 'on disk',
            filePath: A,
            type: 'restoredFileRead',
        });
        expect(next.buffers[0]).toEqual(
            file(A, { content: 'mine', dirty: true, savedContent: 'on disk' }),
        );
    });

    test('a restored read is ignored once the baseline has moved on', () => {
        const saved = file(A, { content: 'mine', savedContent: 'mine' });
        const next = run(state([saved], A), {
            baseline: 'original',
            diskContent: 'stale',
            filePath: A,
            type: 'restoredFileRead',
        });
        expect(next.buffers[0]).toEqual(saved);
    });

    test('reverting replaces a file buffer with the disk content', () => {
        const from = state(
            [file(A, { content: 'mine', dirty: true }), untitled('untitled-1')],
            A,
        );
        const next = run(
            from,
            { bufferId: A, diskContent: 'theirs', type: 'revertToDisk' },
            {
                bufferId: 'untitled-1',
                diskContent: 'theirs',
                type: 'revertToDisk',
            },
        );
        expect(next.buffers).toEqual([
            file(A, { content: 'theirs', savedContent: 'theirs' }),
            untitled('untitled-1'),
        ]);
    });
});

describe('discardUnsavedFiles', () => {
    test('dirty file buffers close; clean files and untitled buffers stay', () => {
        const next = run(
            state(
                [
                    file(A, { dirty: true }),
                    file(B, { dirty: true }),
                    untitled('untitled-1'),
                    file(C),
                ],
                B,
            ),
            { type: 'discardUnsavedFiles' },
        );
        expect(ids(next)).toEqual(['untitled-1', C]);
        expect(next.activeBufferId).toBe('untitled-1');
    });
});
