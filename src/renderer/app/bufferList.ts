import { v4 } from 'uuid';
import type { EditorBuffer } from '../types/editor';
import {
    getBufferId,
    toAbsoluteWorkspacePath,
    withSavedContent,
} from './buffers';

type FileBuffer = EditorBuffer & { kind: 'file' };

export const findBuffer = (
    buffers: EditorBuffer[],
    bufferId: string | undefined,
): EditorBuffer | undefined => buffers.find((b) => getBufferId(b) === bufferId);

export const findFileBuffer = (
    buffers: EditorBuffer[],
    filePath: string | undefined,
): FileBuffer | undefined =>
    buffers.find(
        (b): b is FileBuffer => b.kind === 'file' && b.filePath === filePath,
    );

/** `buffers` with `update` applied to the buffer identified by `bufferId`. */
export const updateBuffer = (
    buffers: EditorBuffer[],
    bufferId: string | undefined,
    update: (buffer: EditorBuffer) => EditorBuffer,
): EditorBuffer[] =>
    buffers.map((b) => (getBufferId(b) === bufferId ? update(b) : b));

/** `buffers` with `update` applied to the buffer open on `filePath`. */
export const updateFileBuffer = (
    buffers: EditorBuffer[],
    filePath: string,
    update: (buffer: FileBuffer) => EditorBuffer,
): EditorBuffer[] =>
    buffers.map((b) =>
        b.kind === 'file' && b.filePath === filePath ? update(b) : b,
    );

export const withoutPreview = (buffer: EditorBuffer): EditorBuffer => ({
    ...buffer,
    isPreview: false,
});

/** A clean buffer for a file just read from disk. */
export const newFileBuffer = (
    filePath: string,
    content: string,
    isPreview: boolean,
): EditorBuffer => ({
    content,
    dirty: false,
    filePath,
    id: v4(),
    isPreview,
    kind: 'file',
    savedContent: content,
});

/**
 * `buffers` with `buffer` opened at the end. A preview buffer takes the place
 * of the current preview unless that one has unsaved edits.
 */
export const withOpenedBuffer = (
    buffers: EditorBuffer[],
    buffer: EditorBuffer,
): EditorBuffer[] => {
    const previewIndex = buffers.findIndex((b) => b.isPreview);
    const kept =
        buffer.isPreview && previewIndex !== -1 && !buffers[previewIndex].dirty
            ? buffers.filter((_, index) => index !== previewIndex)
            : buffers;
    return [...kept, buffer];
};

/**
 * The lowest free `untitled-N` id. A file saved from an untitled buffer keeps
 * its `untitled-N` id, so buffers of every kind reserve their number.
 */
export const nextUntitledId = (buffers: EditorBuffer[]): string => {
    const used = new Set<number>();
    for (const buffer of buffers) {
        const match = buffer.id.match(/^untitled-(\d+)$/);
        if (match) {
            used.add(parseInt(match[1], 10));
        }
    }
    let next = 1;
    while (used.has(next)) {
        next++;
    }
    return `untitled-${next}`;
};

/**
 * `buffers` after the untitled buffer `untitledId` was written to `filePath`
 * with `savedContent`. The buffer becomes a file buffer keeping its `id`. When
 * `filePath` is already open, the untitled buffer folds into that buffer so
 * the path keeps a single buffer identity: two buffers sharing an id would
 * make every id lookup act on whichever comes first.
 */
export const withUntitledSavedAs = (
    buffers: EditorBuffer[],
    untitledId: string,
    filePath: string,
    savedContent: string,
): EditorBuffer[] => {
    const source = findBuffer(buffers, untitledId);
    const alreadyOpen = buffers.some(
        (b) =>
            b.kind === 'file' &&
            b.filePath === filePath &&
            getBufferId(b) !== untitledId,
    );
    if (source && alreadyOpen) {
        return updateFileBuffer(
            buffers.filter((b) => getBufferId(b) !== untitledId),
            filePath,
            (b) =>
                withoutPreview(
                    withSavedContent(
                        { ...b, content: source.content },
                        savedContent,
                    ),
                ),
        );
    }
    return updateBuffer(buffers, untitledId, (b) => ({
        content: b.content,
        dirty: b.content !== savedContent,
        filePath,
        id: b.id,
        kind: 'file',
        savedContent,
    }));
};

/**
 * `buffers` without `bufferId`, and the buffer to activate when the closed one
 * was active: the one that followed it, or the last when it was the tail.
 * Undefined when no buffer has that id.
 */
export const closeBufferIn = (
    buffers: EditorBuffer[],
    bufferId: string,
):
    | { remaining: EditorBuffer[]; nextActiveId: string | undefined }
    | undefined => {
    const index = buffers.findIndex((b) => getBufferId(b) === bufferId);
    if (index === -1) {
        return undefined;
    }
    const remaining = buffers.filter((b) => getBufferId(b) !== bufferId);
    const nextActiveId =
        remaining.length > 0
            ? getBufferId(remaining[Math.min(index, remaining.length - 1)])
            : undefined;
    return { nextActiveId, remaining };
};

/**
 * The file a rename or delete acts on. `target` is a buffer id or a path,
 * absolute or workspace-relative; without one the active buffer's file is
 * used. `isOpen` tells whether a buffer holds the file.
 */
export const resolveFileTarget = (
    buffers: EditorBuffer[],
    activeBufferId: string | undefined,
    workspaceRoot: string | null,
    target: string | undefined,
): { filePath: string; isOpen: boolean } | undefined => {
    const targetPath = target
        ? toAbsoluteWorkspacePath(workspaceRoot, target)
        : undefined;
    const buffer =
        findBuffer(buffers, target) ?? findFileBuffer(buffers, targetPath);
    if (buffer?.kind === 'file') {
        return { filePath: buffer.filePath, isOpen: true };
    }
    if (targetPath) {
        return { filePath: targetPath, isOpen: false };
    }
    const active = findBuffer(buffers, activeBufferId);
    return active?.kind === 'file'
        ? { filePath: active.filePath, isOpen: true }
        : undefined;
};
