/**
 * Positions in a text model that follow the user's edits. They are model
 * decorations, which outlive the editor showing another model and coming
 * back; an editor's own decoration collection is dropped when its model is
 * detached.
 */
import type { editor } from 'monaco-editor';

export interface ModelAnchors {
    /** The current offset of anchor `index`, or null when it has none. */
    offsetOf(index: number): number | null;
    clear(): void;
}

/** One anchor per entry of `positions`, at its 1-based line and column. */
export function createModelAnchors(
    model: editor.ITextModel,
    positions: readonly ({ line: number; column: number } | null)[],
): ModelAnchors {
    const placed = positions.flatMap((p, index) => (p ? [{ index, p }] : []));
    let ids = model.deltaDecorations(
        [],
        placed.map(({ p }) => ({
            options: {
                // NeverGrowsWhenTypingAtEdges
                stickiness: 1,
            },
            range: {
                endColumn: p.column + 1,
                endLineNumber: p.line,
                startColumn: p.column,
                startLineNumber: p.line,
            },
        })),
    );
    const idOf = new Map(placed.map(({ index }, i) => [index, ids[i]]));
    return {
        clear() {
            if (!model.isDisposed()) {
                model.deltaDecorations(ids, []);
            }
            ids = [];
            idOf.clear();
        },
        offsetOf(index) {
            const id = idOf.get(index);
            const range =
                id === undefined || model.isDisposed()
                    ? null
                    : model.getDecorationRange(id);
            return range ? model.getOffsetAt(range.getStartPosition()) : null;
        },
    };
}
