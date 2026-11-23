import { useState, type Dispatch, type SetStateAction } from 'react';
import electronAPI from '../../electronAPI';
import type { EditorBuffer } from '../../types/editor';
import {
    findFileBuffer,
    resolveFileTarget,
    updateFileBuffer,
} from '../bufferList';
import { getBufferId, renamedFilePath } from '../buffers';
import type { EditorViewStates } from '../editorViewStates';
import { fsError } from '../fsResult';

interface UseFileRenameDeleteParams {
    buffers: EditorBuffer[];
    setBuffers: Dispatch<SetStateAction<EditorBuffer[]>>;
    activeBufferId: string | undefined;
    setActiveBufferId: Dispatch<SetStateAction<string | undefined>>;
    workspaceRoot: string | null;
    refreshFileTree: () => Promise<void>;
    viewStates: EditorViewStates;
}

/** Renaming and deleting files on disk, carrying their open buffers along. */
export function useFileRenameDelete({
    buffers,
    setBuffers,
    activeBufferId,
    setActiveBufferId,
    workspaceRoot,
    refreshFileTree,
    viewStates,
}: UseFileRenameDeleteParams) {
    const [renamingPath, setRenamingPath] = useState<string | null>(null);

    const fileTarget = (targetIdOrPath: string | undefined) =>
        resolveFileTarget(
            buffers,
            activeBufferId,
            workspaceRoot,
            targetIdOrPath,
        );

    const renameFile = async (targetIdOrPath?: string) => {
        const target = fileTarget(targetIdOrPath);
        if (target) {
            setRenamingPath(target.filePath);
        }
    };

    const handleRenameCommit = async (oldPath: string, newName: string) => {
        setRenamingPath(null);
        const newPath = renamedFilePath(oldPath, newName);
        if (!newPath) {
            return;
        }

        const result = await electronAPI.filesystem.renameFile(
            oldPath,
            newPath,
        );
        if (!result.success) {
            throw fsError(result, 'Failed to rename file');
        }

        viewStates.move(oldPath, newPath);
        setBuffers((prev) =>
            updateFileBuffer(prev, oldPath, (b) => ({
                ...b,
                filePath: newPath,
            })),
        );
        if (activeBufferId === oldPath) {
            setActiveBufferId(newPath);
        }
        await refreshFileTree();
    };

    const deleteFile = async (targetIdOrPath?: string) => {
        const target = fileTarget(targetIdOrPath);
        if (!target) {
            return;
        }
        const { filePath, isOpen } = target;

        if (!window.confirm(`Delete ${filePath}?`)) {
            return;
        }

        const result = await electronAPI.filesystem.deleteFile(filePath);
        if (!result.success) {
            throw fsError(result, 'Failed to delete file');
        }

        // The updater reports what it removed from the buffers as they are
        // when it runs, which the `buffers` of this render may predate.
        let activeIsDeleted = false;
        let remaining: EditorBuffer[] = [];
        setBuffers((prev) => {
            activeIsDeleted =
                activeBufferId === filePath &&
                (isOpen || findFileBuffer(prev, filePath) !== undefined);
            remaining = prev.filter(
                (b) => !(b.kind === 'file' && b.filePath === filePath),
            );
            return remaining;
        });
        if (activeIsDeleted) {
            setActiveBufferId(
                remaining.length > 0 ? getBufferId(remaining[0]) : undefined,
            );
        }

        await refreshFileTree();
    };

    return {
        deleteFile,
        handleRenameCommit,
        renameFile,
        renamingPath,
        setRenamingPath,
    };
}
