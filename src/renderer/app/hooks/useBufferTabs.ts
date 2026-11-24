import { useState, type Dispatch } from 'react';
import electronAPI from '../../electronAPI';
import type { EditorBuffer } from '../../types/editor';
import { findBuffer, findFileBuffer, newFileBuffer } from '../bufferList';
import type { BufferAction } from '../bufferState';
import { formatBufferLabel, toAbsoluteWorkspacePath } from '../buffers';

/**
 * Reads files from disk. Callers asking for a path while a read of it is in
 * flight share that read.
 */
const createFileReader = () => {
    const pending = new Map<string, Promise<string>>();
    return (absPath: string): Promise<string> => {
        let read = pending.get(absPath);
        if (!read) {
            read = electronAPI.filesystem
                .readFile(absPath)
                .finally(() => pending.delete(absPath));
            pending.set(absPath, read);
        }
        return read;
    };
};

interface UseBufferTabsParams {
    buffers: EditorBuffer[];
    activeBufferId: string | undefined;
    dispatch: Dispatch<BufferAction>;
    workspaceRoot: string | null;
    /** The content a new untitled buffer starts with. */
    defaultPatch: string;
    saveFile: (targetId?: string) => Promise<string | undefined>;
}

/** Opening, editing, previewing and closing the buffers shown as tabs. */
export function useBufferTabs({
    buffers,
    activeBufferId,
    dispatch,
    workspaceRoot,
    defaultPatch,
    saveFile,
}: UseBufferTabsParams) {
    const [readFile] = useState(createFileReader);

    const setActiveBufferId = (bufferId: string) =>
        dispatch({ bufferId, type: 'setActive' });

    const handlePatchChange = (value: string) =>
        dispatch({ bufferId: activeBufferId, content: value, type: 'edit' });

    const keepBuffer = (bufferId: string) =>
        dispatch({ bufferId, type: 'keep' });

    /** Show the buffer open on `absPath`, reading the file if none is. */
    const openPath = async (absPath: string, preview: boolean | undefined) => {
        const keep = preview === false;
        if (findFileBuffer(buffers, absPath)) {
            dispatch({ filePath: absPath, keep, type: 'showFile' });
            return;
        }
        const content = await readFile(absPath);
        dispatch({
            buffer: newFileBuffer(absPath, content, preview ?? false),
            keep,
            type: 'openFile',
        });
    };

    const openFile = async (
        relPath: string,
        options?: { preview?: boolean },
    ) => {
        if (!workspaceRoot) {
            throw new Error('No workspace open');
        }
        await openPath(
            toAbsoluteWorkspacePath(workspaceRoot, relPath),
            options?.preview,
        );
    };

    // Opens a file outside the workspace, such as the user keybindings.json
    // in userData, which has no workspace-relative path.
    const openAbsoluteFile = (absPath: string) => openPath(absPath, undefined);

    const createUntitledFile = () =>
        dispatch({ content: defaultPatch, type: 'createUntitled' });

    const closeBuffer = async (bufferId: string) => {
        const buffer = findBuffer(buffers, bufferId);
        if (!buffer) {
            return;
        }
        if (!buffer.dirty) {
            dispatch({ bufferId, type: 'close' });
            return;
        }

        const response = await electronAPI.showUnsavedChangesDialog(
            formatBufferLabel(buffer),
        );
        if (response === 2) {
            return;
        }
        if (response !== 0) {
            dispatch({ bufferId, type: 'close' });
            return;
        }

        try {
            // Saving an untitled buffer changes its id to the chosen file
            // path; close under the post-save id. A cancelled save dialog
            // aborts the close so the unsaved content is not discarded.
            const savedId = await saveFile(bufferId);
            if (savedId !== undefined) {
                dispatch({ bufferId: savedId, type: 'close' });
            }
        } catch (error) {
            // A failed save aborts the close: the content never reached
            // disk, so the buffer must stay open.
            console.error('Error saving file:', error);
        }
    };

    const discardUnsavedFiles = () => dispatch({ type: 'discardUnsavedFiles' });

    return {
        closeBuffer,
        createUntitledFile,
        discardUnsavedFiles,
        handlePatchChange,
        keepBuffer,
        openAbsoluteFile,
        openFile,
        setActiveBufferId,
    };
}
