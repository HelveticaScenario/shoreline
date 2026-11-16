import { describe, expect, test } from 'vitest';
import type { editor } from 'monaco-editor';
import {
    createEditorViewStates,
    type ViewStateEditor,
} from '../editorViewStates';

/** An editor whose view state is just the line its cursor is on. */
function fakeEditor() {
    const fake = {
        line: 1,
        saveViewState: () =>
            ({ line: fake.line }) as unknown as editor.ICodeEditorViewState,
        restoreViewState: (state: editor.ICodeEditorViewState | null) => {
            fake.line = (state as unknown as { line: number }).line;
        },
    };
    return fake satisfies ViewStateEditor;
}

/** Switches `ed` to `id` as the editor component does: the model swap resets
 *  the view between the two calls. */
function show(
    viewStates: ReturnType<typeof createEditorViewStates>,
    ed: ReturnType<typeof fakeEditor> | null,
    id: string | undefined,
) {
    viewStates.beforeShow(ed, id);
    if (ed) {
        ed.line = 1;
    }
    viewStates.afterShow();
}

describe('createEditorViewStates', () => {
    test('a buffer shown again returns to where it was left', () => {
        const viewStates = createEditorViewStates();
        const ed = fakeEditor();
        show(viewStates, ed, 'a');
        ed.line = 40;
        show(viewStates, ed, 'b');
        expect(ed.line).toBe(1);
        show(viewStates, ed, 'a');
        expect(ed.line).toBe(40);
    });

    test('a shown buffer that moves keeps its view state at the new id', () => {
        const viewStates = createEditorViewStates();
        const ed = fakeEditor();
        show(viewStates, ed, 'untitled-1');
        ed.line = 40;
        viewStates.move('untitled-1', '/w/song.mjs');
        show(viewStates, ed, '/w/song.mjs');
        expect(ed.line).toBe(40);
    });

    test('a move onto an open buffer replaces its view state', () => {
        const viewStates = createEditorViewStates();
        const ed = fakeEditor();
        show(viewStates, ed, '/w/song.mjs');
        ed.line = 10;
        show(viewStates, ed, 'untitled-1');
        ed.line = 40;
        viewStates.move('untitled-1', '/w/song.mjs');
        show(viewStates, ed, '/w/song.mjs');
        expect(ed.line).toBe(40);
    });

    test('a hidden buffer that moves onto the shown one restores its view state', () => {
        const viewStates = createEditorViewStates();
        const ed = fakeEditor();
        show(viewStates, ed, 'untitled-1');
        ed.line = 40;
        show(viewStates, ed, '/w/song.mjs');
        ed.line = 10;
        viewStates.move('untitled-1', '/w/song.mjs');
        // The shown model takes the moved buffer's text, moving the caret.
        ed.line = 1;
        viewStates.afterShow();
        expect(ed.line).toBe(40);
    });

    test('a hidden buffer that moves keeps its view state at the new id', () => {
        const viewStates = createEditorViewStates();
        const ed = fakeEditor();
        show(viewStates, ed, '/w/a.mjs');
        ed.line = 40;
        show(viewStates, ed, '/w/b.mjs');
        viewStates.move('/w/a.mjs', '/w/c.mjs');
        show(viewStates, ed, '/w/c.mjs');
        expect(ed.line).toBe(40);
    });

    test('a closed untitled buffer is forgotten, a closed file is not', () => {
        const viewStates = createEditorViewStates();
        const ed = fakeEditor();
        viewStates.setOpenUntitled(['untitled-1']);
        show(viewStates, ed, 'untitled-1');
        ed.line = 40;
        show(viewStates, ed, '/w/song.mjs');
        ed.line = 20;
        show(viewStates, ed, undefined);
        viewStates.setOpenUntitled([]);

        show(viewStates, ed, 'untitled-1');
        expect(ed.line).toBe(1);
        show(viewStates, ed, '/w/song.mjs');
        expect(ed.line).toBe(20);
    });

    test('a view state waits for an editor that mounts after the switch', () => {
        const viewStates = createEditorViewStates();
        const ed = fakeEditor();
        show(viewStates, ed, 'a');
        ed.line = 40;
        show(viewStates, ed, undefined);
        show(viewStates, null, undefined);

        const next = fakeEditor();
        show(viewStates, null, 'a');
        show(viewStates, next, 'a');
        expect(next.line).toBe(40);
    });
});
