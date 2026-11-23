import { useEffect, useState } from 'react';
import type { OnMount } from '@monaco-editor/react';
import type { editor } from 'monaco-editor';
import { setActiveEditor } from '../../keybindings/dispatch';

type CodeEditor = editor.IStandaloneCodeEditor;

/**
 * Tracks the mounted Monaco editor as state, reports it to `onEditorChange`,
 * and makes it the dispatch target for window-level keybindings and the
 * context menu (see keybindings/dispatch). All go back to `null` when the
 * inner Editor unmounts, which happens when the last buffer closes.
 *
 * No editor-level keybindings are registered on mount. The capture-phase
 * window keymap (keybindings/keymap) owns every shortcut and runs before
 * Monaco, so keybindings.json can rebind or remove each one.
 */
export function useEditorInstance(
    onEditorChange: (editor: CodeEditor | null) => void,
) {
    const [editor, setEditor] = useState<CodeEditor | null>(null);

    useEffect(() => {
        onEditorChange(editor);
        setActiveEditor(editor);
        return () => {
            onEditorChange(null);
            setActiveEditor(null);
        };
    }, [editor, onEditorChange]);

    const handleMount: OnMount = (mounted) => {
        setEditor(mounted);
        // Monaco disposes the editor when the inner Editor unmounts.
        mounted.onDidDispose(() => {
            setEditor((current) => (current === mounted ? null : current));
        });
    };

    return { editor, handleMount };
}
