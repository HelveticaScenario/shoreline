/**
 * A buffer's text and tracked ranges in the shape of a Monaco text model,
 * which the code that anchors scopes and highlights playing steps is written
 * against. Ranges follow edits as Monaco's tracked decorations do, and those
 * carrying a class are drawn as marks.
 */
import {
    type ChangeDesc,
    type EditorState,
    type Extension,
    StateEffect,
    StateField,
} from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view';
import { Range, type editor } from 'monaco-editor';

interface Tracked {
    from: number;
    to: number;
    className?: string;
}

const setMarks = StateEffect.define<DecorationSet>();

const marksField = StateField.define<DecorationSet>({
    create: () => Decoration.none,
    provide: (field) => EditorView.decorations.from(field),
    update(value, tr) {
        for (const effect of tr.effects) {
            if (effect.is(setMarks)) {
                return effect.value;
            }
        }
        return value.map(tr.changes);
    },
});

export class BufferModel {
    private readonly tracked = new Map<string, Tracked>();
    private nextId = 1;
    private disposed = false;
    /** The view while it shows this buffer. */
    private view: EditorView | null = null;

    private state!: EditorState;

    /** Gives the model the state built with its {@link extension}. */
    adopt(state: EditorState): void {
        this.state = state;
    }

    /** The extension a buffer's state carries so its model follows it. */
    extension(): Extension {
        return [
            marksField,
            EditorView.updateListener.of((update) => {
                this.state = update.state;
                if (update.docChanged) {
                    this.map(update.changes);
                }
            }),
        ];
    }

    attach(view: EditorView | null): void {
        this.view = view;
        if (view) {
            this.state = view.state;
            this.draw();
        }
    }

    dispose(): void {
        this.disposed = true;
        this.tracked.clear();
    }

    isDisposed(): boolean {
        return this.disposed;
    }

    getValue(): string {
        return this.state.doc.toString();
    }

    getLineCount(): number {
        return this.state.doc.lines;
    }

    getLineContent(lineNumber: number): string {
        return this.state.doc.line(lineNumber).text;
    }

    getPositionAt(offset: number): { lineNumber: number; column: number } {
        const doc = this.state.doc;
        const line = doc.lineAt(Math.max(0, Math.min(offset, doc.length)));
        return { column: offset - line.from + 1, lineNumber: line.number };
    }

    getOffsetAt(position: { lineNumber: number; column: number }): number {
        const doc = this.state.doc;
        const line = doc.line(
            Math.max(1, Math.min(position.lineNumber, doc.lines)),
        );
        return Math.min(line.to, line.from + position.column - 1);
    }

    getValueInRange(range: {
        startLineNumber: number;
        startColumn: number;
        endLineNumber: number;
        endColumn: number;
    }): string {
        return this.state.sliceDoc(
            this.getOffsetAt({
                column: range.startColumn,
                lineNumber: range.startLineNumber,
            }),
            this.getOffsetAt({
                column: range.endColumn,
                lineNumber: range.endLineNumber,
            }),
        );
    }

    deltaDecorations(
        oldIds: readonly string[],
        added: readonly editor.IModelDeltaDecoration[],
    ): string[] {
        let redraw = false;
        for (const id of oldIds) {
            redraw ||= this.tracked.get(id)?.className !== undefined;
            this.tracked.delete(id);
        }
        const ids = added.map(({ range, options }) => {
            const id = `d${this.nextId++}`;
            const className = options.className ?? undefined;
            redraw ||= className !== undefined;
            this.tracked.set(id, {
                className,
                from: this.getOffsetAt({
                    column: range.startColumn,
                    lineNumber: range.startLineNumber,
                }),
                to: this.getOffsetAt({
                    column: range.endColumn,
                    lineNumber: range.endLineNumber,
                }),
            });
            return id;
        });
        if (redraw) {
            this.draw();
        }
        return ids;
    }

    getDecorationRange(id: string): Range | null {
        const range = this.tracked.get(id);
        if (!range) {
            return null;
        }
        const start = this.getPositionAt(range.from);
        const end = this.getPositionAt(range.to);
        return new Range(
            start.lineNumber,
            start.column,
            end.lineNumber,
            end.column,
        );
    }

    /** Typing at a range's edge leaves the range as it was. */
    private map(changes: ChangeDesc): void {
        for (const range of this.tracked.values()) {
            const from = changes.mapPos(range.from, 1);
            range.to = Math.max(from, changes.mapPos(range.to, -1));
            range.from = from;
        }
    }

    private draw(): void {
        if (!this.view) {
            return;
        }
        const marks = [...this.tracked.values()]
            .filter((range) => range.className && range.to > range.from)
            .sort((a, b) => a.from - b.from || a.to - b.to)
            .map((range) =>
                Decoration.mark({ class: range.className }).range(
                    range.from,
                    range.to,
                ),
            );
        this.view.dispatch({ effects: setMarks.of(Decoration.set(marks)) });
    }
}

let shown: BufferModel | null = null;

/** The model of the buffer the editor shows, as a Monaco text model. */
export function shownBufferModel(): editor.ITextModel | null {
    return shown as unknown as editor.ITextModel | null;
}

export function setShownBufferModel(model: BufferModel | null): void {
    shown = model;
}
