import { useEffect, useReducer, useState } from 'react';
import type { QuoteStyle } from '../../../shared/ipcTypes';
import { requote } from '../../dsl/requote';
import { findBuffer } from '../bufferList';
import { bufferReducer, initialBufferState } from '../bufferState';
import {
    DEFAULT_PATCH,
    readUnsavedBuffers,
    saveUnsavedBuffers,
} from '../buffers';
import { createEditorViewStates } from '../editorViewStates';
import { useBufferDiskSync } from './useBufferDiskSync';
import { useBufferSaving } from './useBufferSaving';
import { useBufferTabs } from './useBufferTabs';
import { useFileRenameDelete } from './useFileRenameDelete';

interface UseEditorBuffersParams {
    workspaceRoot: string | null;
    refreshFileTree: () => Promise<void>;
    /** Called with the absolute path after a buffer is successfully saved. */
    onFileSaved?: (filePath: string) => void;
    /** The quote style a new untitled buffer's patch is written in. */
    quotes: QuoteStyle;
}

export function useEditorBuffers({
    workspaceRoot,
    refreshFileTree,
    onFileSaved,
    quotes,
}: UseEditorBuffersParams) {
    const defaultPatch = requote(DEFAULT_PATCH, quotes);
    const [restoredBuffers] = useState(readUnsavedBuffers);
    const [{ buffers, activeBufferId }, dispatch] = useReducer(
        bufferReducer,
        restoredBuffers,
        initialBufferState,
    );

    const patchCode =
        findBuffer(buffers, activeBufferId)?.content ?? defaultPatch;

    useEffect(() => {
        saveUnsavedBuffers(buffers);
    }, [buffers]);

    const [viewStates] = useState(createEditorViewStates);
    const untitledIdsKey = JSON.stringify(
        buffers.flatMap((b) => (b.kind === 'untitled' ? [b.id] : [])),
    );
    useEffect(() => {
        viewStates.setOpenUntitled(JSON.parse(untitledIdsKey) as string[]);
    }, [untitledIdsKey, viewStates]);

    useBufferDiskSync(buffers, restoredBuffers, dispatch);

    const shared = { activeBufferId, buffers, dispatch, workspaceRoot };
    const saving = useBufferSaving({
        ...shared,
        onFileSaved,
        refreshFileTree,
        viewStates,
    });
    const files = useFileRenameDelete({
        ...shared,
        refreshFileTree,
        viewStates,
    });
    const tabs = useBufferTabs({
        ...shared,
        defaultPatch,
        saveFile: saving.saveFile,
    });

    return {
        ...saving,
        ...files,
        ...tabs,
        activeBufferId,
        buffers,
        patchCode,
        viewStates,
    };
}
