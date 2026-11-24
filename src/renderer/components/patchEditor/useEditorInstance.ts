import { useState } from 'react';
import type { OnMount } from '@monaco-editor/react';
import type { editor } from 'monaco-editor';
import { getActiveEditor, setActiveEditor } from '../../keybindings/dispatch';

type CodeEditor = editor.IStandaloneCodeEditor;

/**
 * Owns the mounted Monaco editor. It publishes the editor, and `null` once
 * Monaco disposes it (the inner Editor unmounts when the last buffer
 * closes), to its two kinds of reader at the same moment: `onEditorChange`
 * for React, and the dispatch target of keybindings/dispatch for the
 * window-level keymap and the context menu. Both happen inside Monaco's own
 * callbacks, so a reader that runs before the next render already sees the
 * editor. The returned `editor` is the same value as state, for the
 * component's own hooks.
 *
 * No editor-level keybindings are registered on mount. The capture-phase
 * window keymap (keybindings/keymap) owns every shortcut and runs before
 * Monaco, so keybindings.json can rebind or remove each one.
 */
export function useEditorInstance(
    onEditorChange: (editor: CodeEditor | null) => void,
) {
    const [editor, setEditor] = useState<CodeEditor | null>(null);

    const handleMount: OnMount = (mounted) => {
        const publish = (next: CodeEditor | null) => {
            setActiveEditor(next);
            setEditor(next);
            onEditorChange(next);
        };
        publish(mounted);
        mounted.onDidDispose(() => {
            if (getActiveEditor() === mounted) {
                publish(null);
            }
        });
    };

    return { editor, handleMount };
}
