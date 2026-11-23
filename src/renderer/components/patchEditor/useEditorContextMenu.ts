import { useEffect } from 'react';
import type { editor } from 'monaco-editor';
import electronAPI from '../../electronAPI';
import { buildEditorMenuItems } from '../../keybindings/editorMenuItems';
import { dispatchCommand } from '../../keybindings/dispatch';

/**
 * Pops a native Electron menu, built from the command registry, on
 * right-click and dispatches the chosen item through the shared command
 * path. Monaco's built-in menu is disabled in the editor options.
 */
export function useEditorContextMenu(
    editor: editor.IStandaloneCodeEditor | null,
) {
    useEffect(() => {
        if (!editor) {
            return;
        }
        const contextSub = editor.onContextMenu((e) => {
            // Focus on every right-click (editor convention) so the native
            // clipboard roles act on this editor.
            editor.focus();
            const position = e.target.position;
            const selection = editor.getSelection();
            // Right-clicking outside the current selection collapses the caret
            // to the click point so paste lands where the user clicked; an
            // existing selection is preserved so copy/cut act on it.
            if (
                position &&
                (!selection || !selection.containsPosition(position))
            ) {
                editor.setPosition(position);
            }
            void electronAPI.showContextMenu({
                type: 'editor',
                items: buildEditorMenuItems(editor),
            });
        });
        const disposeCommandSub = electronAPI.onContextMenuCommand((action) => {
            if (action.command !== 'editor' || !action.commandId) {
                return;
            }
            // Registry commands run via the registry; editor actions and core
            // editor commands are triggered on this editor. Clipboard roles
            // never round-trip — they are handled natively in the main process.
            // Refocus first: the native menu took focus, and editor commands
            // (Go to Definition, etc.) need the editor focused to act.
            editor.focus();
            dispatchCommand(action.commandId);
        });
        return () => {
            contextSub.dispose();
            disposeCommandSub();
        };
    }, [editor]);
}
