import { useEffect, useEffectEvent } from 'react';
import electronAPI from '../../electronAPI';
import type { EditorBuffer } from '../../types/editor';
import { getErrorMessage } from '../../utils/errorUtils';
import { getBufferId } from '../buffers';
import type { useEditorBuffers } from './useEditorBuffers';

type Buffers = ReturnType<typeof useEditorBuffers>;

/**
 * The file commands as the UI runs them: each reports its failure through
 * `showError` and never rejects.
 */
export function useFileActions({
    files,
    chooseWorkspace,
    ensureKeybindingsFile,
    showError,
}: {
    files: Buffers;
    chooseWorkspace: () => Promise<void>;
    /** Resolves to the keybindings file's path, creating the file if needed. */
    ensureKeybindingsFile: () => Promise<string>;
    showError: (message: string) => void;
}) {
    /** Run `action`, reporting a failure as `fallback` when the error has
     *  no message of its own. */
    const guarded = async (
        action: () => Promise<unknown>,
        fallback: string,
    ) => {
        try {
            await action();
        } catch (err) {
            showError(getErrorMessage(err, fallback));
        }
    };

    const save = (id?: string) =>
        guarded(() => files.saveFile(id), 'Failed to save file');
    const rename = (target?: string) =>
        guarded(() => files.renameFile(target), 'Failed to rename file');
    const remove = (target?: string) =>
        guarded(() => files.deleteFile(target), 'Failed to delete file');

    const open = async (relPath: string, options?: { preview?: boolean }) => {
        try {
            await files.openFile(relPath, options);
        } catch {
            showError(`Failed to open file: ${relPath}`);
        }
    };

    const commitRename = (oldPath: string, newName: string) =>
        guarded(
            () => files.handleRenameCommit(oldPath, newName),
            'Failed to rename file',
        );

    /** Open the keybindings file as a buffer; saving it reloads the keymap. */
    const openKeybindings = () =>
        guarded(
            async () => files.openAbsoluteFile(await ensureKeybindingsFile()),
            'Failed to open keybindings',
        );

    /**
     * Switch workspace folder, first settling the open files' unsaved
     * changes with the user. A file that fails to save or hits a save
     * conflict keeps the current workspace open.
     */
    const switchWorkspace = async () => {
        const dirty = files.buffers.filter(
            (b): b is Extract<EditorBuffer, { kind: 'file' }> =>
                b.kind === 'file' && b.dirty,
        );
        if (dirty.length > 0) {
            const response = await electronAPI.showUnsavedChangesDialog(
                dirty.map((b) => b.filePath).join(', '),
            );
            if (response === 2) {
                // Cancel
                return;
            }
            if (response === 0) {
                // Save
                for (const buffer of dirty) {
                    try {
                        const saved = await files.saveFile(getBufferId(buffer));
                        if (saved === undefined) {
                            return;
                        }
                    } catch (err) {
                        showError(getErrorMessage(err, 'Failed to save file'));
                        return;
                    }
                }
            } else {
                // Don't Save
                files.setBuffers((prev) =>
                    prev.filter((b) => !(b.kind === 'file' && b.dirty)),
                );
            }
        }
        await chooseWorkspace();
    };

    const onContextMenuCommand = useEffectEvent(
        (
            action: Parameters<
                Parameters<typeof electronAPI.onContextMenuCommand>[0]
            >[0],
        ) => {
            switch (action.command) {
                case 'save':
                    void save(action.bufferId);
                    break;
                case 'rename':
                    void rename(action.path || action.bufferId);
                    break;
                case 'delete':
                    void remove(action.path || action.bufferId);
                    break;
            }
        },
    );
    useEffect(
        () => electronAPI.onContextMenuCommand((a) => onContextMenuCommand(a)),
        [],
    );

    return {
        commitRename,
        open,
        openKeybindings,
        remove,
        save,
        switchWorkspace,
    };
}
