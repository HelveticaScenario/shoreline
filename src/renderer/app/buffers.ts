import type { EditorBuffer, UnsavedBufferSnapshot } from '../types/editor';
import {
    isPatchPath,
    PATCH_EXTENSION,
    patchExtension,
} from '../../shared/patchFiles';

export const DEFAULT_PATCH = `// Simple 440 Hz sine wave
keys: $sine('a3')
`;

const UNSAVED_STORAGE_KEY = 'modular_unsaved_buffers';

export const readUnsavedBuffers = (): EditorBuffer[] => {
    if (typeof window === 'undefined') {
        return [];
    }

    try {
        const raw = window.localStorage.getItem(UNSAVED_STORAGE_KEY);
        if (!raw) {
            return [];
        }

        const parsed = JSON.parse(raw) as UnsavedBufferSnapshot[];
        return parsed.map((snapshot): EditorBuffer => {
            if (snapshot.kind === 'file') {
                const savedContent = snapshot.savedContent ?? null;
                return {
                    content: snapshot.content,
                    dirty:
                        savedContent === null ||
                        snapshot.content !== savedContent,
                    filePath: snapshot.filePath,
                    id: snapshot.id,
                    kind: 'file',
                    savedContent,
                };
            }
            return {
                content: snapshot.content,
                dirty: true,
                id: snapshot.id,
                kind: 'untitled',
            };
        });
    } catch (error) {
        console.error('Failed to read unsaved buffers:', error);
        return [];
    }
};

export const saveUnsavedBuffers = (buffers: EditorBuffer[]) => {
    if (typeof window === 'undefined') {
        return;
    }

    try {
        const dirtyBuffers = buffers.filter((b) => b.dirty);
        const snapshots: UnsavedBufferSnapshot[] = dirtyBuffers.map(
            (buffer) => {
                if (buffer.kind === 'file') {
                    return {
                        content: buffer.content,
                        filePath: buffer.filePath,
                        id: buffer.filePath,
                        kind: 'file',
                        ...(buffer.savedContent !== null && {
                            savedContent: buffer.savedContent,
                        }),
                    };
                }
                return {
                    content: buffer.content,
                    id: buffer.id,
                    kind: 'untitled',
                };
            },
        );

        window.localStorage.setItem(
            UNSAVED_STORAGE_KEY,
            JSON.stringify(snapshots),
        );
    } catch (error) {
        console.error('Failed to save unsaved buffers:', error);
    }
};

/**
 * Replace a buffer's content. A file buffer is dirty only while its content
 * differs from what is on disk; an untitled buffer is dirty once edited.
 */
export const withBufferContent = (
    buffer: EditorBuffer,
    content: string,
): EditorBuffer =>
    buffer.kind === 'file'
        ? {
              ...buffer,
              content,
              dirty:
                  buffer.savedContent === null ||
                  content !== buffer.savedContent,
          }
        : { ...buffer, content, dirty: true };

/** Record that `savedContent` is what the buffer's file now holds on disk. */
export const withSavedContent = (
    buffer: EditorBuffer & { kind: 'file' },
    savedContent: string,
): EditorBuffer => ({
    ...buffer,
    dirty: buffer.content !== savedContent,
    savedContent,
});

/**
 * Apply a change to a file buffer's file on disk. A clean buffer takes the new
 * content; a dirty one keeps its edits, and its next save reports a conflict.
 */
export const withDiskChange = (
    buffer: EditorBuffer & { kind: 'file' },
    diskContent: string,
): EditorBuffer => {
    if (buffer.content === diskContent || !buffer.dirty) {
        return withSavedContent(
            { ...buffer, content: diskContent },
            diskContent,
        );
    }
    return buffer;
};

export const getBufferId = (buffer: EditorBuffer): string =>
    buffer.kind === 'file' ? buffer.filePath : buffer.id;

/**
 * File-buffer identity is the absolute path (see `getBufferId`), but several
 * IPC surfaces hand back workspace-relative paths (the save dialog, the file
 * tree, context menus). Resolve through here before a path is stored or
 * compared, so every surface produces the same identity for the same file.
 * Already-absolute paths — POSIX or Windows drive-letter — pass through
 * unchanged; the drive check requires a separator after the colon so a
 * relative name like 'c:song.sl' (legal on macOS/Linux) still resolves
 * against the workspace.
 */
export const toAbsoluteWorkspacePath = (
    workspaceRoot: string | null,
    path: string,
): string => {
    if (
        workspaceRoot &&
        !path.startsWith('/') &&
        !/^[a-zA-Z]:[\\/]/.test(path)
    ) {
        return `${workspaceRoot}/${path}`;
    }
    return path;
};

export const formatBufferLabel = (buffer: EditorBuffer) => {
    if (buffer.kind === 'untitled') {
        return buffer.id;
    }
    return buffer.filePath;
};

/** `name` trimmed, with `extension` added unless it already names a patch. */
export const normalizeFileName = (
    name: string,
    extension: string = PATCH_EXTENSION,
) => {
    const trimmed = name.trim();
    if (!trimmed) {
        return trimmed;
    }
    return isPatchPath(trimmed) ? trimmed : `${trimmed}${extension}`;
};

/**
 * The name a rename gives `oldPath`'s file. A patch keeps its own extension
 * when the new name has none; any other file takes the new name as typed.
 */
export const renamedFileName = (oldPath: string, newName: string) => {
    const extension = patchExtension(oldPath);
    return extension === undefined
        ? newName.trim()
        : normalizeFileName(newName, extension);
};

/**
 * The path a rename to `newName` gives `oldPath`'s file, in the same folder.
 * Undefined when the name is empty or names the file as it is.
 */
export const renamedFilePath = (
    oldPath: string,
    newName: string,
): string | undefined => {
    if (!newName || newName === oldPath.split(/[/\\]/).pop()) {
        return undefined;
    }
    const fileName = renamedFileName(oldPath, newName);
    const separator = oldPath.includes('\\') ? '\\' : '/';
    const lastSeparator = oldPath.lastIndexOf(separator);
    const newPath =
        lastSeparator === -1
            ? fileName
            : `${oldPath.substring(0, lastSeparator)}${separator}${fileName}`;
    return newPath && newPath !== oldPath ? newPath : undefined;
};
