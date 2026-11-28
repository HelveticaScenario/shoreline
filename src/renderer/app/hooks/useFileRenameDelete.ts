import { useState, type Dispatch } from 'react';
import electronAPI from '../../electronAPI';
import type { EditorBuffer } from '../../types/editor';
import { fsError } from '../../utils/errorUtils';
import { resolveFileTarget } from '../bufferList';
import type { BufferAction } from '../bufferState';
import { renamedFilePath, toAbsoluteWorkspacePath } from '../buffers';
import type { EditorViewStates } from '../editorViewStates';

interface UseFileRenameDeleteParams {
    buffers: EditorBuffer[];
    activeBufferId: string | undefined;
    dispatch: Dispatch<BufferAction>;
    workspaceRoot: string | null;
    refreshFileTree: () => Promise<void>;
    viewStates: EditorViewStates;
}

/** The path `filePath` takes when the file or folder `oldPath` moves to `newPath`. */
const movedPath = (filePath: string, oldPath: string, newPath: string) => {
    if (filePath === oldPath) return newPath;
    const rest = filePath.slice(oldPath.length);
    return rest.startsWith('/') || rest.startsWith('\\')
        ? newPath + rest
        : undefined;
};

/** Renaming, moving and deleting files on disk, carrying their open buffers along. */
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

        carryBuffers(oldPath, newPath);
        await refreshFileTree();
    };

    /** Moves the workspace-relative entry `fromPath` to `toPath`. */
    const moveEntry = async (fromPath: string, toPath: string) => {
        const result = await electronAPI.filesystem.moveFile(fromPath, toPath);
        if (!result.success) {
            throw fsError(result, 'Failed to move file');
        }

        carryBuffers(
            toAbsoluteWorkspacePath(workspaceRoot, fromPath),
            toAbsoluteWorkspacePath(workspaceRoot, toPath),
        );
        await refreshFileTree();
    };

    /**
     * Points the buffers of `oldPath`, or of any file under it, at `newPath`.
     * The view state of `oldPath` itself moves even while it has no buffer,
     * so a closed file reopens where it was left.
     */
    const carryBuffers = (oldPath: string, newPath: string) => {
        viewStates.move(oldPath, newPath);
        for (const buffer of buffers) {
            if (buffer.kind !== 'file') continue;
            const moved = movedPath(buffer.filePath, oldPath, newPath);
            if (moved) {
                if (moved !== newPath) {
                    viewStates.move(buffer.filePath, moved);
                }
                dispatch({
                    newPath: moved,
                    oldPath: buffer.filePath,
                    type: 'renameFile',
                });
            }
        }
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
        moveEntry,
        renameFile,
        renamingPath,
        setRenamingPath,
    };
}
