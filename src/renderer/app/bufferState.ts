import type { EditorBuffer, FileBuffer } from '../types/editor';
import {
    findBuffer,
    findFileBuffer,
    nextUntitledId,
    updateBuffer,
    updateFileBuffer,
    withOpenedBuffer,
    withoutPreview,
    withUntitledSavedAs,
} from './bufferList';
import {
    getBufferId,
    withBufferContent,
    withDiskChange,
    withSavedContent,
} from './buffers';

/**
 * The open buffers and which one the editor shows. `activeBufferId` is a
 * `getBufferId` of one of `buffers`, or undefined when none is shown.
 */
export interface BufferState {
    buffers: EditorBuffer[];
    activeBufferId: string | undefined;
}

export type BufferAction =
    /**
     * Open a file just read from disk and show it. A path that is already
     * open keeps its buffer, as `showFile` would.
     */
    | { type: 'openFile'; buffer: FileBuffer; keep: boolean }
    /** Show the buffer open on a path; `keep` ends its preview. */
    | { type: 'showFile'; filePath: string; keep: boolean }
    /** Open and show a new untitled buffer holding `content`. */
    | { type: 'createUntitled'; content: string }
    | { type: 'setActive'; bufferId: string }
    /** Replace a buffer's content with what was typed; ends its preview. */
    | { type: 'edit'; bufferId: string | undefined; content: string }
    /** End a buffer's preview so that opening another leaves it open. */
    | { type: 'keep'; bufferId: string }
    | { type: 'close'; bufferId: string }
    /** Close the buffer of a file removed from disk. */
    | { type: 'deleteFile'; filePath: string }
    /** Carry the buffer of a file moved on disk to its new path. */
    | { type: 'renameFile'; oldPath: string; newPath: string }
    /** An untitled buffer was written to `filePath` as `savedContent`. */
    | {
          type: 'saveUntitledAs';
          untitledId: string;
          filePath: string;
          savedContent: string;
      }
    /** A file buffer's `savedContent` reached disk. */
    | { type: 'fileSaved'; filePath: string; savedContent: string }
    /** A file changed on disk outside the editor. */
    | { type: 'diskChanged'; filePath: string; content: string }
    /**
     * The file of a buffer restored from storage was read. `baseline` is the
     * buffer's `savedContent` when the read began; the read is stale once a
     * save or reload has replaced it.
     */
    | {
          type: 'restoredFileRead';
          filePath: string;
          baseline: string | null;
          diskContent: string;
      }
    /** Replace a file buffer's content with what its file holds. */
    | { type: 'revertToDisk'; bufferId: string; diskContent: string }
    /** Close every file buffer with unsaved changes, dropping the changes. */
    | { type: 'discardUnsavedFiles' };

export const initialBufferState = (buffers: EditorBuffer[]): BufferState => ({
    activeBufferId: buffers.length > 0 ? getBufferId(buffers[0]) : undefined,
    buffers,
});

/**
 * `state` without the buffers `isRemoved` picks. When the active buffer is
 * among them, the buffer that followed it becomes active, or the last one
 * left when none followed.
 */
const withoutBuffers = (
    state: BufferState,
    isRemoved: (buffer: EditorBuffer) => boolean,
): BufferState => {
    const buffers = state.buffers.filter((b) => !isRemoved(b));
    if (buffers.length === state.buffers.length) {
        return state;
    }
    const activeIndex = state.buffers.findIndex(
        (b) => getBufferId(b) === state.activeBufferId,
    );
    if (activeIndex === -1 || !isRemoved(state.buffers[activeIndex])) {
        return { ...state, buffers };
    }
    if (buffers.length === 0) {
        return { activeBufferId: undefined, buffers };
    }
    const keptBefore = state.buffers
        .slice(0, activeIndex)
        .filter((b) => !isRemoved(b)).length;
    const next = buffers[Math.min(keptBefore, buffers.length - 1)];
    return { activeBufferId: getBufferId(next), buffers };
};

const showFile = (
    state: BufferState,
    filePath: string,
    keep: boolean,
): BufferState => {
    if (!findFileBuffer(state.buffers, filePath)) {
        return state;
    }
    return {
        activeBufferId: filePath,
        buffers: keep
            ? updateFileBuffer(state.buffers, filePath, withoutPreview)
            : state.buffers,
    };
};

export const bufferReducer = (
    state: BufferState,
    action: BufferAction,
): BufferState => {
    const { buffers, activeBufferId } = state;
    switch (action.type) {
        case 'openFile': {
            const { filePath } = action.buffer;
            if (findFileBuffer(buffers, filePath)) {
                return showFile(state, filePath, action.keep);
            }
            return {
                activeBufferId: filePath,
                buffers: withOpenedBuffer(buffers, action.buffer),
            };
        }
        case 'showFile':
            return showFile(state, action.filePath, action.keep);
        case 'createUntitled': {
            const id = nextUntitledId(buffers);
            return {
                activeBufferId: id,
                buffers: [
                    ...buffers,
                    {
                        content: action.content,
                        dirty: false,
                        id,
                        kind: 'untitled',
                    },
                ],
            };
        }
        case 'setActive':
            return findBuffer(buffers, action.bufferId)
                ? { ...state, activeBufferId: action.bufferId }
                : state;
        case 'edit':
            return {
                ...state,
                buffers: updateBuffer(buffers, action.bufferId, (b) =>
                    withoutPreview(withBufferContent(b, action.content)),
                ),
            };
        case 'keep':
            return {
                ...state,
                buffers: updateBuffer(buffers, action.bufferId, withoutPreview),
            };
        case 'close':
            return withoutBuffers(
                state,
                (b) => getBufferId(b) === action.bufferId,
            );
        case 'deleteFile':
            return withoutBuffers(
                state,
                (b) => b.kind === 'file' && b.filePath === action.filePath,
            );
        case 'renameFile':
            if (!findFileBuffer(buffers, action.oldPath)) {
                return state;
            }
            return {
                activeBufferId:
                    activeBufferId === action.oldPath
                        ? action.newPath
                        : activeBufferId,
                buffers: updateFileBuffer(buffers, action.oldPath, (b) => ({
                    ...b,
                    filePath: action.newPath,
                })),
            };
        case 'saveUntitledAs':
            if (!findBuffer(buffers, action.untitledId)) {
                return state;
            }
            return {
                activeBufferId:
                    activeBufferId === action.untitledId
                        ? action.filePath
                        : activeBufferId,
                buffers: withUntitledSavedAs(
                    buffers,
                    action.untitledId,
                    action.filePath,
                    action.savedContent,
                ),
            };
        case 'fileSaved':
            return {
                ...state,
                buffers: updateFileBuffer(buffers, action.filePath, (b) =>
                    withSavedContent(b, action.savedContent),
                ),
            };
        case 'diskChanged':
            return {
                ...state,
                buffers: updateFileBuffer(buffers, action.filePath, (b) =>
                    withDiskChange(b, action.content),
                ),
            };
        case 'restoredFileRead':
            // A buffer whose baseline is unknown adopts the disk content as
            // its baseline; one with a baseline treats the disk content like
            // an external change, so edits made against an older file still
            // conflict on save.
            return {
                ...state,
                buffers: updateFileBuffer(buffers, action.filePath, (b) => {
                    if (b.savedContent !== action.baseline) {
                        return b;
                    }
                    return b.savedContent === null
                        ? withSavedContent(b, action.diskContent)
                        : withDiskChange(b, action.diskContent);
                }),
            };
        case 'revertToDisk':
            return {
                ...state,
                buffers: updateBuffer(buffers, action.bufferId, (b) =>
                    b.kind === 'file'
                        ? withSavedContent(
                              { ...b, content: action.diskContent },
                              action.diskContent,
                          )
                        : b,
                ),
            };
        case 'discardUnsavedFiles':
            return withoutBuffers(state, (b) => b.kind === 'file' && b.dirty);
    }
};
