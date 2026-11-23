import { useState, type Dispatch } from 'react';
import electronAPI from '../../electronAPI';
import type { EditorBuffer } from '../../types/editor';
import { fsError } from '../../utils/errorUtils';
import { resolveFileTarget } from '../bufferList';
import type { BufferAction } from '../bufferState';
import { renamedFilePath } from '../buffers';
import type { EditorViewStates } from '../editorViewStates';

interface UseFileRenameDeleteParams {
    buffers: EditorBuffer[];
    activeBufferId: string | undefined;
    dispatch: Dispatch<BufferAction>;
    workspaceRoot: string | null;
    refreshFileTree: () => Promise<void>;
    viewStates: EditorViewStates;
}

/** Renaming and deleting files on disk, carrying their open buffers along. */
export function useFileRenameDelete({
    buffers,
    activeBufferId,
    dispatch,
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
        const filePath = fileTarget(targetIdOrPath);
        if (filePath) {
            setRenamingPath(filePath);
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
        dispatch({ newPath, oldPath, type: 'renameFile' });
        await refreshFileTree();
    };

    const deleteFile = async (targetIdOrPath?: string) => {
        const filePath = fileTarget(targetIdOrPath);
        if (!filePath) {
            return;
        }

        if (!window.confirm(`Delete ${filePath}?`)) {
            return;
        }

        const result = await electronAPI.filesystem.deleteFile(filePath);
        if (!result.success) {
            throw fsError(result, 'Failed to delete file');
        }

        dispatch({ filePath, type: 'deleteFile' });
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
