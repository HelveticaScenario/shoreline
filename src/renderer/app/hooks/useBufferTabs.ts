import { useEffect, useRef, type Dispatch, type SetStateAction } from 'react';
import electronAPI from '../../electronAPI';
import type { EditorBuffer } from '../../types/editor';
import {
    closeBufferIn,
    findBuffer,
    findFileBuffer,
    newFileBuffer,
    nextUntitledId,
    updateBuffer,
    withOpenedBuffer,
    withoutPreview,
} from '../bufferList';
import {
    formatBufferLabel,
    toAbsoluteWorkspacePath,
    withBufferContent,
} from '../buffers';

/** How long a close waits, so state updates made by a save have flushed. */
const CLOSE_DELAY_MS = 50;

interface UseBufferTabsParams {
    buffers: EditorBuffer[];
    setBuffers: Dispatch<SetStateAction<EditorBuffer[]>>;
    activeBufferId: string | undefined;
    setActiveBufferId: Dispatch<SetStateAction<string | undefined>>;
    workspaceRoot: string | null;
    /** The content a new untitled buffer starts with. */
    defaultPatch: string;
    saveFile: (targetId?: string) => Promise<string | undefined>;
}

/** Opening, editing, previewing and closing the buffers shown as tabs. */
export function useBufferTabs({
    buffers,
    setBuffers,
    activeBufferId,
    setActiveBufferId,
    workspaceRoot,
    defaultPatch,
    saveFile,
}: UseBufferTabsParams) {
    const handlePatchChange = (value: string) => {
        setBuffers((prev) =>
            updateBuffer(prev, activeBufferId, (b) =>
                withoutPreview(withBufferContent(b, value)),
            ),
        );
    };

    const keepBuffer = (bufferId: string) => {
        setBuffers((prev) => updateBuffer(prev, bufferId, withoutPreview));
    };

    /** Activate the buffer open on `absPath`, reading the file if none is. */
    const openPath = async (absPath: string, preview: boolean | undefined) => {
        const existing = findFileBuffer(buffers, absPath);
        if (existing) {
            if (preview === false && existing.isPreview) {
                keepBuffer(absPath);
            }
            setActiveBufferId(absPath);
            return;
        }

        const content = await electronAPI.filesystem.readFile(absPath);
        setBuffers((prev) =>
            withOpenedBuffer(
                prev,
                newFileBuffer(absPath, content, preview ?? false),
            ),
        );
        setActiveBufferId(absPath);
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

    const createUntitledFile = () => {
        setBuffers((prev) => {
            const id = nextUntitledId(prev);
            setActiveBufferId(id);
            return [
                ...prev,
                { content: defaultPatch, dirty: false, id, kind: 'untitled' },
            ];
        });
    };

    // The active id as of the latest render, for the deferred close: it runs
    // after a save that re-identified the buffer has rendered.
    const activeBufferIdRef = useRef(activeBufferId);
    useEffect(() => {
        activeBufferIdRef.current = activeBufferId;
    });

    const performCloseBuffer = (bufferId: string) => {
        setTimeout(() => {
            const currentActiveId = activeBufferIdRef.current;
            setBuffers((prev) => {
                const closed = closeBufferIn(prev, bufferId);
                if (!closed) {
                    return prev;
                }
                if (currentActiveId === bufferId) {
                    setActiveBufferId(closed.nextActiveId);
                }
                return closed.remaining;
            });
        }, CLOSE_DELAY_MS);
    };

    const closeBuffer = async (bufferId: string) => {
        const buffer = findBuffer(buffers, bufferId);
        if (!buffer) {
            return;
        }
        if (!buffer.dirty) {
            performCloseBuffer(bufferId);
            return;
        }

        const response = await electronAPI.showUnsavedChangesDialog(
            formatBufferLabel(buffer),
        );
        if (response === 2) {
            return;
        }
        if (response !== 0) {
            performCloseBuffer(bufferId);
            return;
        }

        try {
            // Saving an untitled buffer changes its id to the chosen file
            // path; close under the post-save id. A cancelled save dialog
            // aborts the close so the unsaved content is not discarded.
            const savedId = await saveFile(bufferId);
            if (savedId !== undefined) {
                performCloseBuffer(savedId);
            }
        } catch (error) {
            // A failed save aborts the close: the content never reached
            // disk, so the buffer must stay open.
            console.error('Error saving file:', error);
        }
    };

    return {
        closeBuffer,
        createUntitledFile,
        handlePatchChange,
        keepBuffer,
        openAbsoluteFile,
        openFile,
    };
}
