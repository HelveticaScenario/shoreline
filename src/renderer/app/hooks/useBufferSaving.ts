import { useState, type Dispatch, type SetStateAction } from 'react';
import electronAPI from '../../electronAPI';
import type { EditorBuffer, FileBuffer } from '../../types/editor';
import { PATCH_EXTENSION } from '../../../shared/patchFiles';
import {
    findBuffer,
    updateFileBuffer,
    withUntitledSavedAs,
} from '../bufferList';
import {
    getBufferId,
    normalizeFileName,
    toAbsoluteWorkspacePath,
    withSavedContent,
} from '../buffers';
import type { EditorViewStates } from '../editorViewStates';
import { fsError } from '../fsResult';

const SAVE_FAILED = 'Failed to save file';

interface UseBufferSavingParams {
    buffers: EditorBuffer[];
    setBuffers: Dispatch<SetStateAction<EditorBuffer[]>>;
    activeBufferId: string | undefined;
    setActiveBufferId: Dispatch<SetStateAction<string | undefined>>;
    workspaceRoot: string | null;
    refreshFileTree: () => Promise<void>;
    onFileSaved?: (filePath: string) => void;
    viewStates: EditorViewStates;
}

/** Writing buffers to disk, and the conflict a refused write leaves open. */
export function useBufferSaving({
    buffers,
    setBuffers,
    activeBufferId,
    setActiveBufferId,
    workspaceRoot,
    refreshFileTree,
    onFileSaved,
    viewStates,
}: UseBufferSavingParams) {
    /** A save refused because the file changed on disk since it was read. */
    const [saveConflict, setSaveConflict] = useState<{
        bufferId: string;
        filePath: string;
    } | null>(null);

    const saveUntitled = async (buffer: EditorBuffer) => {
        const savedContent = buffer.content;
        const input = await electronAPI.filesystem.showSaveDialog(
            `untitled${PATCH_EXTENSION}`,
        );
        const normalized = input ? normalizeFileName(input) : '';
        if (!normalized) {
            return undefined;
        }

        // The save dialog only ever returns workspace-relative paths.
        const filePath = toAbsoluteWorkspacePath(workspaceRoot, normalized);
        const result = await electronAPI.filesystem.writeFile(
            filePath,
            savedContent,
        );
        if (!result.success) {
            throw fsError(result, SAVE_FAILED);
        }

        viewStates.move(buffer.id, filePath);
        setBuffers((prev) =>
            withUntitledSavedAs(prev, buffer.id, filePath, savedContent),
        );
        if (buffer.id === activeBufferId) {
            setActiveBufferId(filePath);
        }
        await refreshFileTree();
        onFileSaved?.(filePath);
        return filePath;
    };

    const saveToFile = async (
        buffer: FileBuffer,
        overwrite: boolean | undefined,
    ) => {
        const { filePath, content: savedContent } = buffer;
        const result = await electronAPI.filesystem.writeFile(
            filePath,
            savedContent,
            overwrite ? undefined : (buffer.savedContent ?? undefined),
        );
        if (result.success) {
            setBuffers((prev) =>
                updateFileBuffer(prev, filePath, (b) =>
                    withSavedContent(b, savedContent),
                ),
            );
            onFileSaved?.(filePath);
            return filePath;
        }
        if ('conflict' in result) {
            setSaveConflict({ bufferId: getBufferId(buffer), filePath });
            return undefined;
        }
        throw fsError(result, SAVE_FAILED);
    };

    /**
     * Save a buffer to disk. Returns the buffer's id after the save (the
     * absolute file path — saving an untitled buffer changes its id), or
     * undefined when nothing was saved (buffer missing, dialog cancelled, or
     * a save conflict).
     *
     * A file that changed on disk since the buffer last read or wrote it is
     * not overwritten: the save is refused and `saveConflict` is set, unless
     * `overwrite` is passed.
     *
     * Edits can land while the async write is in flight, so the buffer is
     * compared against the snapshot that reached disk, not marked clean.
     */
    const saveFile = async (
        targetId?: string,
        options?: { overwrite?: boolean },
    ): Promise<string | undefined> => {
        const buffer = findBuffer(buffers, targetId || activeBufferId);
        if (!buffer) {
            return undefined;
        }
        return buffer.kind === 'untitled'
            ? saveUntitled(buffer)
            : saveToFile(buffer, options?.overwrite);
    };

    const dismissSaveConflict = () => setSaveConflict(null);

    /** Resolve a save conflict by writing the buffer over the file on disk. */
    const overwriteOnConflict = async (bufferId: string) => {
        setSaveConflict(null);
        await saveFile(bufferId, { overwrite: true });
    };

    /** Resolve a save conflict by replacing the buffer with the disk content. */
    const revertToDiskContent = (bufferId: string, diskContent: string) => {
        setSaveConflict(null);
        setBuffers((prev) =>
            updateFileBuffer(prev, bufferId, (b) =>
                withSavedContent({ ...b, content: diskContent }, diskContent),
            ),
        );
    };

    return {
        dismissSaveConflict,
        overwriteOnConflict,
        revertToDiskContent,
        saveConflict,
        saveFile,
    };
}
