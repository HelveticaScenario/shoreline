import type { editor } from 'monaco-editor';
import type { ModelAnchors } from '../app/modelAnchors';
import type { TrackSourceEdit } from '../dsl/trackSourceEdit';
import type { MixerStore } from './mixerStore';

/** Builds a track's source edit from the document, the offset of the
 *  track's label, and the track's name. */
export type MakeTrackEdit = (
    source: string,
    anchor: number,
    name: string,
) => TrackSourceEdit | null;

/** Interval between live source writes while a control is dragging. */
const EDIT_INTERVAL_MS = 150;

/**
 * Mirrors mixer control changes into the DSL source of the buffer the
 * mixer's tracks were built from, at each track's tracked label.
 */
export class MixerSourceEdits {
    private editor: editor.IStandaloneCodeEditor | null = null;
    /** Anchors on each track's label, index-aligned with the store's
     *  outputs; they follow the label as the user edits around it. */
    private anchors: ModelAnchors | null = null;
    /** The buffer the tracks were built from. It stays through a stop,
     *  while the mixer still shows those tracks. */
    private tracksSourceId: string | null = null;
    /** The buffer the editor shows: edits reach the source only while it
     *  is the one the tracks came from. */
    private visibleBufferId: string | undefined = undefined;
    /** Throttle state for live writes during drags, keyed
     *  `<meterKey>:<control>`. Each Monaco edit re-tokenizes and re-colors,
     *  so a drag writes immediately, then at most once per interval. */
    private readonly throttles = new Map<
        string,
        { timer: number | null; lastWrite: number }
    >();

    constructor(private readonly store: MixerStore) {}

    attachEditor(instance: editor.IStandaloneCodeEditor | null): void {
        this.editor = instance;
    }

    setVisibleBuffer(bufferId: string | undefined): void {
        this.visibleBufferId = bufferId;
    }

    /** Take over an applied patch's label anchors and source buffer. */
    setSource(anchors: ModelAnchors | null, sourceId: string | null): void {
        this.anchors?.clear();
        this.anchors = anchors;
        this.tracksSourceId = sourceId;
    }

    /**
     * Offset of the label of the track named `key` in the editor's source;
     * 0 for the main strip, which has no label. Null when the editor shows
     * another buffer than the tracks' own.
     */
    labelOffset(key: string): number | null {
        const idx = this.store.outputs.findIndex((o) => o.key === key);
        if (
            idx === -1 ||
            this.tracksSourceId === null ||
            this.visibleBufferId !== this.tracksSourceId
        ) {
            return null;
        }
        return this.store.outputs[idx].main
            ? 0
            : (this.anchors?.offsetOf(idx) ?? null);
    }

    /** Stable id of the buffer the tracks were built from. */
    get sourceId(): string | null {
        return this.tracksSourceId;
    }

    /**
     * Write the edit `makeEdit` builds for the track named `key`. Returns
     * whether an edit was written; skipped when the editor shows another
     * buffer than the tracks' own, or when the track has no editable form
     * for it (a non-literal value, a renamed label).
     */
    apply(key: string, makeEdit: MakeTrackEdit): boolean {
        const model = this.editor?.getModel();
        // The main strip's anchor is 0: its edit finds its own place.
        const anchor = this.labelOffset(key);
        if (!model || anchor === null) {
            return false;
        }
        const edit = makeEdit(model.getValue(), anchor, key);
        if (!edit) {
            return false;
        }
        this.push(model, edit);
        if (edit.dropsTrack) {
            // The label's value is its expression again, so later option
            // edits append a `.track(...)` to it.
            this.store.updateOutput(key, (o) => ({ ...o, fromTrack: false }));
        }
        return true;
    }

    /** Throttled live write for a dragging control: the leading edge fires
     *  immediately, further moves at most once per interval. */
    schedule(timerKey: string, write: () => void): void {
        let state = this.throttles.get(timerKey);
        if (!state) {
            state = { lastWrite: 0, timer: null };
            this.throttles.set(timerKey, state);
        }
        if (state.timer !== null) {
            window.clearTimeout(state.timer);
            state.timer = null;
        }
        const elapsed = performance.now() - state.lastWrite;
        if (elapsed >= EDIT_INTERVAL_MS) {
            state.lastWrite = performance.now();
            write();
            return;
        }
        const pending = state;
        pending.timer = window.setTimeout(() => {
            pending.timer = null;
            pending.lastWrite = performance.now();
            write();
        }, EDIT_INTERVAL_MS - elapsed);
    }

    /** Cancel a pending throttled write (its final flush is imminent). */
    cancel(timerKey: string): void {
        const state = this.throttles.get(timerKey);
        if (state?.timer != null) {
            window.clearTimeout(state.timer);
            state.timer = null;
        }
    }

    /** Apply a character-offset edit, keeping the user's selections in
     *  place (the default parks the cursor at the end of the edited span)
     *  and the edit on the undo stack. */
    private push(
        model: editor.ITextModel,
        edit: { start: number; end: number; text: string },
    ): void {
        const start = model.getPositionAt(edit.start);
        const end = model.getPositionAt(edit.end);
        const selections = this.editor?.getSelections() ?? null;
        model.pushEditOperations(
            selections ?? [],
            [
                {
                    range: {
                        endColumn: end.column,
                        endLineNumber: end.lineNumber,
                        startColumn: start.column,
                        startLineNumber: start.lineNumber,
                    },
                    text: edit.text,
                },
            ],
            () => selections,
        );
    }
}
