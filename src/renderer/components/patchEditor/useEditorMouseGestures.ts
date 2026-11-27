import { useEffect, useEffectEvent } from 'react';
import type { editor } from 'monaco-editor';
import electronAPI from '../../electronAPI';
import { buildEditorMenuItems } from '../../keybindings/editorMenuItems';
import { dispatchCommand } from '../../keybindings/dispatch';
import { installWorkspacePathDrop } from '../monaco/workspacePathCompletion';
import { hasDefinitionDocs } from '../monaco/definitionDocs';
import type { EditorCodeFormat } from './codeFormat';

interface EditorMouseGesturesParams {
    editor: editor.IStandaloneCodeEditor | null;
    /** Whether the module schemas have loaded; peeking a symbol waits for them. */
    dslReady: boolean;
    codeFormat: EditorCodeFormat;
}

/**
 * The mouse gestures the editor adds to Monaco's own: a right-click opens
 * the native context menu, Cmd/Ctrl + click opens a name's docs or peeks its
 * definition, and a file dragged from the explorer drops in as its path.
 */
export function useEditorMouseGestures({
    editor,
    dslReady,
    codeFormat,
}: EditorMouseGesturesParams) {
    const currentFormat = useEffectEvent(() => codeFormat);

    // A native Electron menu, built from the command registry, whose chosen
    // item goes through the shared command path. Monaco's built-in menu is
    // disabled in the editor options.
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

    // Cmd (Mac) / Ctrl (Win/Linux) + click peeks a definition no docs cover.
    useEffect(() => {
        if (!editor || !dslReady) {
            return;
        }
        const disposable = editor.onMouseDown((e) => {
            if (!e.event.metaKey && !e.event.ctrlKey) {
                return;
            }
            const position = e.target.position;
            const model = editor.getModel();
            if (position == null || !model) {
                return;
            }
            editor.focus();
            // Monaco's own Cmd/Ctrl + click goes to the definition, which
            // opens the docs of a name they cover; anything else peeks.
            void hasDefinitionDocs(model, position).then((docs) => {
                if (!docs) {
                    editor.trigger('api', 'editor.action.peekDefinition', {});
                }
            });
        });
        return () => disposable.dispose();
    }, [editor, dslReady]);

    useEffect(() => {
        if (!editor) {
            return;
        }
        const drop = installWorkspacePathDrop(
            editor,
            () => currentFormat().quote,
        );
        return () => drop.dispose();
    }, [editor]);
}
