/**
 * Scopes as panels under their calls: block widgets holding the canvases the
 * scope renderer draws into.
 */
import {
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
} from '@codemirror/view';
import type { ModelRanges } from '../../app/modelAnchors';
import type { ScopeView } from '../../types/editor';

const SCOPE_HEIGHT = 80;
const SPLIT_GAP = 4;

export interface ScopeCanvasRegistry {
    register?: (key: string, canvas: HTMLCanvasElement) => void;
    unregister?: (key: string) => void;
}

class ScopeWidget extends WidgetType {
    private observer: ResizeObserver | null = null;
    private keys: string[] = [];

    constructor(
        private readonly view: ScopeView,
        private readonly registry: ScopeCanvasRegistry,
    ) {
        super();
    }

    eq(other: ScopeWidget): boolean {
        return other.view.key === this.view.key;
    }

    get estimatedHeight(): number {
        return this.height();
    }

    private specs() {
        const { view } = this;
        return view.split && view.laneKeys.length > 1
            ? view.laneKeys.map((laneKey, i) => ({
                  key: `${view.key}#${i}`,
                  laneKeys: [laneKey],
              }))
            : [{ key: view.key, laneKeys: view.laneKeys }];
    }

    private height(): number {
        const count = this.specs().length;
        return this.view.split === 'vertical'
            ? count * SCOPE_HEIGHT + (count - 1) * SPLIT_GAP
            : SCOPE_HEIGHT;
    }

    toDOM(): HTMLElement {
        const { view } = this;
        const stacked = view.split === 'vertical';
        const container = document.createElement('div');
        container.className = 'scope-view-zone';
        Object.assign(container.style, {
            display: 'flex',
            flexDirection: stacked ? 'column' : 'row',
            gap: `${SPLIT_GAP}px`,
            height: `${this.height()}px`,
        });
        const resize = (canvas: HTMLCanvasElement) => {
            const dpr = window.devicePixelRatio || 1;
            const width = Math.max(1, Math.floor(canvas.clientWidth * dpr));
            const height = Math.max(1, Math.floor(SCOPE_HEIGHT * dpr));
            // Assigning a canvas dimension clears it, so only on a change.
            if (canvas.width !== width) {
                canvas.width = width;
            }
            if (canvas.height !== height) {
                canvas.height = height;
            }
        };
        this.observer = new ResizeObserver((entries) => {
            for (const entry of entries) {
                resize(entry.target as HTMLCanvasElement);
            }
        });
        for (const spec of this.specs()) {
            const canvas = container.appendChild(
                document.createElement('canvas'),
            );
            Object.assign(canvas.style, {
                flex: '1 1 0',
                height: `${SCOPE_HEIGHT}px`,
                minWidth: '0',
                width: stacked ? '100%' : undefined,
            });
            canvas.dataset.scopeKey = spec.key;
            canvas.dataset.scopeRangeMin = String(view.range[0]);
            canvas.dataset.scopeRangeMax = String(view.range[1]);
            canvas.dataset.scopeLaneKeys = JSON.stringify(spec.laneKeys);
            this.observer.observe(canvas);
            this.keys.push(spec.key);
            this.registry.register?.(spec.key, canvas);
        }
        return container;
    }

    destroy(): void {
        this.observer?.disconnect();
        for (const key of this.keys) {
            this.registry.unregister?.(key);
        }
        this.keys = [];
    }

    ignoreEvent(): boolean {
        return true;
    }
}

const setScopes = StateEffect.define<DecorationSet>();

const scopeField = StateField.define<DecorationSet>({
    create: () => Decoration.none,
    provide: (field) => EditorView.decorations.from(field),
    update(value, tr) {
        for (const effect of tr.effects) {
            if (effect.is(setScopes)) {
                return effect.value;
            }
        }
        // A panel stays under its call as edits move the call's line.
        return value.map(tr.changes);
    },
});

export function scopeZones(): Extension {
    return scopeField;
}

/** Shows `views`, each under the last line of its call. */
export function showScopes(
    editorView: EditorView,
    views: readonly ScopeView[],
    anchors: ModelRanges | null,
    registry: ScopeCanvasRegistry,
): void {
    const doc = editorView.state.doc;
    const widgets: Range<Decoration>[] = [];
    for (const view of views) {
        const range =
            view.decorationIndex === null
                ? null
                : anchors?.getRange(view.decorationIndex);
        if (!range || range.isEmpty() || range.endLineNumber > doc.lines) {
            continue;
        }
        widgets.push(
            Decoration.widget({
                block: true,
                side: 1,
                widget: new ScopeWidget(view, registry),
            }).range(doc.line(range.endLineNumber).to),
        );
    }
    editorView.dispatch({
        effects: setScopes.of(Decoration.set(widgets, true)),
    });
}
