import { useEffect } from 'react';
import type { editor } from 'monaco-editor';
import type { PrettierConfig } from '../../../shared/ipcTypes';
import {
    bindEditorContextConstants,
    bindEditorFocus,
    bindEditorWidgetVisibility,
} from '../../keybindings/contextKeyBootstrap';
import { codeStyleFromPrettier } from '../../dsl/listEdits';
import { trackCodeBacking } from '../monaco/codeBacking';
import {
    DEFAULT_PRETTIER_OPTIONS,
    resolvePrettierOptions,
} from '../monaco/formattingProvider';
import { installWorkspacePathDrop } from '../monaco/workspacePathCompletion';

interface EditorBehavioursParams {
    editor: editor.IStandaloneCodeEditor | null;
    /** Whether the DSL's definitions are loaded, so a symbol can be peeked. */
    dslReady: boolean;
    prettierConfig: PrettierConfig;
}

/** Wires the behaviours that belong to the editor instance itself. */
export function useEditorBehaviours({
    editor,
    dslReady,
    prettierConfig,
}: EditorBehavioursParams) {
    // Mirror Monaco focus and suggest/find widget visibility into the
    // context-key service so when-clauses can react.
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

    // Cmd (Mac) / Ctrl (Win/Linux) + click peeks a DSL symbol's definition.
    useEffect(() => {
        if (!editor || !dslReady) {
            return;
        }
        const disposable = editor.onMouseDown((e) => {
            if (!e.event.metaKey && !e.event.ctrlKey) {
                return;
            }
            if (e.target.position == null || !editor.getModel()) {
                return;
            }
            editor.focus();
            editor.trigger('api', 'editor.action.peekDefinition', {});
        });
        return () => disposable.dispose();
    }, [editor, dslReady]);

    const tabSize =
        prettierConfig.tabWidth ?? DEFAULT_PRETTIER_OPTIONS.tabWidth;
    useEffect(() => {
        if (!editor) {
            return;
        }
        const apply = () => {
            editor.getModel()?.updateOptions({ insertSpaces: true, tabSize });
        };
        apply();
        const disposable = editor.onDidChangeModel(apply);
        return () => disposable.dispose();
    }, [editor, tabSize]);

    useEffect(() => {
        if (!editor) {
            return;
        }
        return trackCodeBacking(editor);
    }, [editor]);

    const { quote } = codeStyleFromPrettier(
        resolvePrettierOptions(prettierConfig),
    );
    useEffect(() => {
        if (!editor) {
            return;
        }
        const drop = installWorkspacePathDrop(editor, () => quote);
        return () => drop.dispose();
    }, [editor, quote]);
}
