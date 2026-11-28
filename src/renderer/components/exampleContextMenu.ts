import type { editor } from 'monaco-editor';
import electronAPI from '../electronAPI';
import { buildReadOnlyEditorMenuItems } from '../keybindings/editorMenuItems';

/** The example editor whose context menu is open, which its choice acts on. */
let owner: editor.ICodeEditor | null = null;
/** The menu's choices reach every listener, so one serves all examples. */
let listening = false;

/** Opens the native context menu of a docs example's editor. */
export function showExampleContextMenu(ed: editor.ICodeEditor): void {
    owner = ed;
    if (!listening) {
        listening = true;
        electronAPI.onContextMenuCommand((action) => {
            if (
                action.command !== 'docsExample' ||
                !action.commandId ||
                !owner
            ) {
                return;
            }
            // The native menu took focus; editor actions act on a focused
            // editor.
            owner.focus();
            owner.trigger('contextmenu', action.commandId, null);
        });
    }
    void electronAPI.showContextMenu({
        items: buildReadOnlyEditorMenuItems(ed),
        target: 'docsExample',
        type: 'editor',
    });
}

/** Forgets `ed` as the menu's owner once it is disposed. */
export function forgetExampleEditor(ed: editor.ICodeEditor): void {
    if (owner === ed) {
        owner = null;
    }
}
