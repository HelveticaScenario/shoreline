/**
 * A peek: locations shown in a panel under the line they were asked from,
 * with a read-only view of the code at the selected one. It shows a
 * definition inside the DSL's declarations and the references to a name.
 */
import { javascript } from '@codemirror/lang-javascript';
import {
    EditorState,
    type Extension,
    type Range,
    StateEffect,
    StateField,
} from '@codemirror/state';
import {
    Decoration,
    type DecorationSet,
    EditorView,
    WidgetType,
    keymap,
    lineNumbers,
} from '@codemirror/view';

export interface PeekLocation {
    /** What the list shows for the location. */
    label: string;
    /** The whole text the location is in. */
    text: string;
    from: number;
    to: number;
    /** Moves the main editor there; absent for text outside the buffer. */
    jump?: () => void;
}

interface Peek {
    /** Offset in the buffer the peek hangs under. */
    pos: number;
    title: string;
    locations: PeekLocation[];
    /** The extensions the preview shares with the editor: its look. */
    look: Extension;
}

export const showPeek = StateEffect.define<Peek | null>();

class PeekWidget extends WidgetType {
    private preview: EditorView | null = null;

    constructor(private readonly peek: Peek) {
        super();
    }

    eq(other: PeekWidget): boolean {
        return other.peek === this.peek;
    }

    toDOM(view: EditorView): HTMLElement {
        const { peek } = this;
        const dom = document.createElement('div');
        dom.className = 'cm-peek';
        const header = dom.appendChild(document.createElement('div'));
        header.className = 'cm-peek-header';
        header.append(peek.title);
        const close = header.appendChild(document.createElement('button'));
        close.textContent = '×';
        close.title = 'Close (Escape)';
        close.onclick = () => {
            view.dispatch({ effects: showPeek.of(null) });
            view.focus();
        };

        const body = dom.appendChild(document.createElement('div'));
        body.className = 'cm-peek-body';
        const previewHost = body.appendChild(document.createElement('div'));
        previewHost.className = 'cm-peek-preview';
        const list = body.appendChild(document.createElement('ul'));
        list.className = 'cm-peek-list';
        list.hidden = peek.locations.length < 2;

        const select = (index: number) => {
            const location = peek.locations[index];
            this.preview?.destroy();
            this.preview = new EditorView({
                parent: previewHost,
                state: EditorState.create({
                    doc: location.text,
                    extensions: [
                        EditorState.readOnly.of(true),
                        lineNumbers(),
                        javascript({ typescript: true }),
                        peek.look,
                        EditorView.decorations.of(
                            Decoration.set([
                                Decoration.mark({
                                    class: 'cm-peek-target',
                                }).range(
                                    location.from,
                                    Math.max(location.to, location.from + 1),
                                ),
                            ]),
                        ),
                    ],
                    selection: { anchor: location.from },
                }),
            });
            this.preview.dispatch({
                effects: EditorView.scrollIntoView(location.from, {
                    y: 'center',
                }),
            });
            [...list.children].forEach((item, i) =>
                item.classList.toggle('selected', i === index),
            );
        };

        peek.locations.forEach((location, index) => {
            const item = list.appendChild(document.createElement('li'));
            item.textContent = location.label;
            item.onclick = () => select(index);
            item.ondblclick = () => {
                if (location.jump) {
                    view.dispatch({ effects: showPeek.of(null) });
                    location.jump();
                }
            };
        });
        // The preview measures itself, which needs the panel in the document.
        requestAnimationFrame(() => select(0));
        return dom;
    }

    destroy(): void {
        this.preview?.destroy();
    }

    ignoreEvent(): boolean {
        return true;
    }
}

const peekField = StateField.define<DecorationSet>({
    create: () => Decoration.none,
    provide: (field) => EditorView.decorations.from(field),
    update(value, tr) {
        for (const effect of tr.effects) {
            if (effect.is(showPeek)) {
                if (!effect.value) {
                    return Decoration.none;
                }
                const line = tr.state.doc.lineAt(effect.value.pos);
                const widget: Range<Decoration> = Decoration.widget({
                    block: true,
                    side: 1,
                    widget: new PeekWidget(effect.value),
                }).range(line.to);
                return Decoration.set([widget]);
            }
        }
        return value.map(tr.changes);
    },
});

export function peekPanel(): Extension {
    return [
        peekField,
        keymap.of([
            {
                key: 'Escape',
                run(view) {
                    if (view.state.field(peekField).size === 0) {
                        return false;
                    }
                    view.dispatch({ effects: showPeek.of(null) });
                    return true;
                },
            },
        ]),
    ];
}
