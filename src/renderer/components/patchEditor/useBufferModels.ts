import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { editor } from 'monaco-editor';
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { EditorViewStates } from '../../app/editorViewStates';
import { formatPath } from '../monaco/monacoHelpers';
import { createModelValueSync } from '../monaco/modelValueSync';

interface BufferModelsParams {
    editor: editor.IStandaloneCodeEditor | null;
    monaco: Monaco | null;
    value: string;
    currentFile: string | undefined;
    openFiles: string[];
    viewStates: EditorViewStates;
    /** Columns per indent level of every buffer the editor shows. */
    tabSize: number;
    onChange: (value: string) => void;
}

/**
 * Replaces the model's text as one undoable edit. `writing.current` is true
 * for the duration, so the change event the edit fires is recognised as the
 * write and not as typing.
 */
function writeModelText(
    editor: editor.IStandaloneCodeEditor,
    model: editor.ITextModel,
    text: string,
    writing: { current: boolean },
) {
    writing.current = true;
    try {
        editor.executeEdits('', [
            {
                forceMoveMarkers: true,
                range: model.getFullModelRange(),
                text,
            },
        ]);
        editor.pushUndoStop();
    } finally {
        writing.current = false;
    }
}

/**
 * Keeps one Monaco model per open buffer and the shown buffer's text, view
 * state and indentation in step with the app. Returns the Editor's change
 * handler.
 */
export function useBufferModels({
    editor,
    monaco,
    value,
    currentFile,
    openFiles,
    viewStates,
    tabSize,
    onChange,
}: BufferModelsParams) {
    // Each buffer keeps its own model (keepCurrentModel) so undo history
    // survives switching. A closed buffer's model must be disposed: buffer ids
    // are reused (a new untitled takes the lowest free number, a deleted file
    // can be recreated), and the Editor adopts any existing model at a path
    // over the buffer's content. Runs after the Editor's own effects, so the
    // editor has already moved off a disposed model.
    const openModelPathsRef = useRef(new Set<string>());
    useEffect(() => {
        if (!monaco) {
            return;
        }
        const paths = new Set(openFiles.map(formatPath));
        for (const path of openModelPathsRef.current) {
            if (!paths.has(path)) {
                monaco.editor.getModel(monaco.Uri.parse(path))?.dispose();
            }
        }
        openModelPathsRef.current = paths;
    }, [monaco, openFiles]);

    // View states live in `viewStates`, which follows buffers whose ids
    // change, so the Editor's own store is off. A buffer's view state is saved
    // here, before the Editor's passive effect swaps models, and restored
    // below once it has.
    useLayoutEffect(() => {
        viewStates.beforeShow(editor, currentFile);
    }, [editor, currentFile, viewStates]);

    // The model, not `value`, is the source of truth for the active buffer:
    // `value` is written into it only when it is text the editor did not
    // emit, so a value that arrives after newer keystrokes never replaces
    // them (or moves the caret). The Editor gets `defaultValue` alone, which
    // seeds a model it creates for a new path.
    const [valueSync] = useState(createModelValueSync);
    const syncedModelRef = useRef<editor.ITextModel | null>(null);
    const writingValueRef = useRef(false);
    const handleEditorChange = (text: string | undefined) => {
        if (writingValueRef.current) {
            return;
        }
        valueSync.noteEmitted(text ?? '');
        onChange(text ?? '');
    };
    useEffect(() => {
        const model = editor?.getModel();
        if (!editor || !model) {
            return;
        }
        if (model !== syncedModelRef.current) {
            syncedModelRef.current = model;
            valueSync.reset();
        }
        if (valueSync.shouldWrite(value, model.getValue())) {
            writeModelText(editor, model, value, writingValueRef);
        }
    }, [editor, value, currentFile, valueSync]);

    // After the value write above, which moves the caret when a model adopts
    // another buffer's text (an untitled buffer saved over an open file).
    useEffect(() => {
        viewStates.afterShow();
    }, [editor, currentFile, value, viewStates]);

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

    return handleEditorChange;
}
