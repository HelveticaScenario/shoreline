import type { editor } from 'monaco-editor';

type ViewState = editor.ICodeEditorViewState;

/** The part of a Monaco editor whose view state is tracked. */
export type ViewStateEditor = Pick<
    editor.ICodeEditor,
    'saveViewState' | 'restoreViewState'
>;

/**
 * Each buffer's cursor, selections, scroll and folds, keyed by buffer id
 * (`getBufferId`), kept while the editor shows other buffers.
 *
 * An untitled buffer's view state is dropped when it closes, so a new buffer
 * that reuses its id opens at the top; a file's is kept, so a reopened file
 * returns to where it was left. A buffer whose id changes (saved to a file,
 * renamed) carries its view state to the new id.
 */
export interface EditorViewStates {
    /**
     * Call while the editor still shows its previous buffer (a layout
     * effect), before it switches to `bufferId`: stores the previous buffer's
     * view state.
     */
    beforeShow(
        editor: ViewStateEditor | null,
        bufferId: string | undefined,
    ): void;
    /**
     * Call once the editor shows the new buffer's model, and again whenever
     * its text is written (passive effects): a move onto the shown buffer
     * replaces its text along with its view state.
     */
    afterShow(): void;
    /** Carries `fromId`'s view state to `toId`, replacing any `toId` had. */
    move(fromId: string, toId: string): void;
    /** Ids of the open untitled buffers; ones no longer listed are dropped. */
    setOpenUntitled(ids: Iterable<string>): void;
}

export function createEditorViewStates(): EditorViewStates {
    const states = new Map<string, ViewState>();
    let shownEditor: ViewStateEditor | null = null;
    let shownId: string | undefined;
    let restorePending = false;
    let openUntitled = new Set<string>();

    const storeShown = (id: string) => {
        const state = shownEditor?.saveViewState();
        if (state) {
            states.set(id, state);
        }
    };

    return {
        beforeShow(editor, bufferId) {
            if (editor === shownEditor && bufferId === shownId) {
                return;
            }
            if (editor === shownEditor && shownId !== undefined) {
                storeShown(shownId);
            }
            shownEditor = editor;
            shownId = bufferId;
            restorePending = true;
        },
        afterShow() {
            if (!restorePending || !shownEditor || shownId === undefined) {
                return;
            }
            restorePending = false;
            const state = states.get(shownId);
            if (state) {
                shownEditor.restoreViewState(state);
            }
        },
        move(fromId, toId) {
            if (fromId === toId) {
                return;
            }
            if (fromId === shownId && shownEditor) {
                // The editor still shows the buffer under its old id and
                // switches models once the move renders.
                storeShown(toId);
                shownId = toId;
                restorePending = true;
            } else {
                const state = states.get(fromId);
                if (state) {
                    states.set(toId, state);
                } else {
                    states.delete(toId);
                }
                if (toId === shownId) {
                    restorePending = true;
                }
            }
            states.delete(fromId);
        },
        setOpenUntitled(ids) {
            const open = new Set(ids);
            for (const id of openUntitled) {
                if (!open.has(id)) {
                    states.delete(id);
                }
            }
            openUntitled = open;
        },
    };
}
