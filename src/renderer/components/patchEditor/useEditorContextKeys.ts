import { useEffect } from 'react';
import type { editor } from 'monaco-editor';
import {
    bindEditorContextConstants,
    bindEditorFocus,
    bindEditorWidgetVisibility,
} from '../../keybindings/contextKeyBootstrap';

/**
 * Mirrors the editor's focus, its suggest and find widgets' visibility and
 * its constants into the context-key service, which the `when` clauses of
 * keybindings read.
 */
export function useEditorContextKeys(
    editor: editor.IStandaloneCodeEditor | null,
) {
    useEffect(() => {
        if (!editor) {
            return;
        }
        const stopFocus = bindEditorFocus(editor);
        const stopWidgets = bindEditorWidgetVisibility(editor);
        const stopConstants = bindEditorContextConstants(editor);
        return () => {
            stopFocus();
            stopWidgets();
            stopConstants();
        };
    }, [editor]);
}
