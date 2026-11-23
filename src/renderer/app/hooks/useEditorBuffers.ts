import { useEffect, useState } from 'react';
import type { QuoteStyle } from '../../../shared/ipcTypes';
import { requote } from '../../dsl/requote';
import { findBuffer } from '../bufferList';
import {
    DEFAULT_PATCH,
    formatBufferLabel,
    getBufferId,
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
    const [buffers, setBuffers] = useState(restoredBuffers);

    const [activeBufferId, setActiveBufferId] = useState<string | undefined>(
        () =>
            restoredBuffers.length > 0
                ? getBufferId(restoredBuffers[0])
                : undefined,
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

    useBufferDiskSync(buffers, restoredBuffers, setBuffers);

    const shared = {
        activeBufferId,
        buffers,
        setActiveBufferId,
        setBuffers,
        workspaceRoot,
    };
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
        formatFileLabel: formatBufferLabel,
        patchCode,
        setActiveBufferId,
        setBuffers,
        viewStates,
    };
}
