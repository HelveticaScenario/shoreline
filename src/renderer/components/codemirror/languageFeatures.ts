/**
 * The language server's answers as editor features: completion, hover,
 * diagnostics, signature help and occurrence highlights.
 */
import {
    type Completion,
    type CompletionContext,
    type CompletionResult,
    autocompletion,
} from '@codemirror/autocomplete';
import { type Diagnostic, linter } from '@codemirror/lint';
import {
    type EditorState,
    type Extension,
    StateEffect,
    StateField,
} from '@codemirror/state';
import {
    Decoration,
    type DecorationSet,
    EditorView,
    type Tooltip,
    ViewPlugin,
    type ViewUpdate,
    hoverTooltip,
    showTooltip,
} from '@codemirror/view';
import {
    type LanguageClient,
    type LspRange,
    fromLspRange,
    toLspPosition,
} from './languageClient';
import { markdownElement, markupText } from './markdown';

interface LspCompletionItem {
    label: string;
    kind?: number;
    detail?: string;
    labelDetails?: { detail?: string; description?: string };
    documentation?: { value: string } | string;
    sortText?: string;
    filterText?: string;
    insertText?: string;
    textEdit?: { newText: string; range?: LspRange; insert?: LspRange };
}

const COMPLETION_TYPES: Record<number, string> = {
    2: 'method',
    3: 'function',
    4: 'class',
    5: 'property',
    6: 'variable',
    7: 'class',
    8: 'interface',
    9: 'namespace',
    10: 'property',
    12: 'constant',
    13: 'enum',
    14: 'keyword',
    20: 'constant',
    21: 'constant',
    22: 'class',
    25: 'type',
};

/** Names the editor's own declarations add, which a patch never writes. */
const isEditorInternal = (label: string) => label.startsWith('__');

function completionSource(client: LanguageClient, uri: string) {
    return async (
        context: CompletionContext,
    ): Promise<CompletionResult | null> => {
        const word = context.matchBefore(/[\w$]*/);
        const before = context.state.sliceDoc(
            Math.max(0, context.pos - 1),
            context.pos,
        );
        const from = word ? word.from : context.pos;
        const afterDot =
            context.state.sliceDoc(Math.max(0, from - 1), from) === '.';
        if (!context.explicit && from === context.pos && !afterDot) {
            return null;
        }
        const result = await client.request<
            { items: LspCompletionItem[] } | LspCompletionItem[]
        >('textDocument/completion', {
            context:
                before === '.'
                    ? { triggerCharacter: '.', triggerKind: 2 }
                    : { triggerKind: 1 },
            position: toLspPosition(context.state.doc, context.pos),
            textDocument: { uri },
        });
        if (!result || context.aborted) {
            return null;
        }
        const items = Array.isArray(result) ? result : result.items;
        const options = items
            .filter((item) => !isEditorInternal(item.label))
            .map((item, index): Completion => ({
                apply: item.textEdit?.newText ?? item.insertText,
                // The server's order, which ranks by scope and relevance.
                boost: -index / items.length,
                detail:
                    item.labelDetails?.description ?? item.labelDetails?.detail,
                info: async () => {
                    const resolved: LspCompletionItem | null =
                        await client.request('completionItem/resolve', item);
                    const signature = resolved?.detail
                        ? '```typescript\n' + resolved.detail + '\n```\n\n'
                        : '';
                    const text =
                        signature + markupText(resolved?.documentation);
                    return text ? markdownElement(text, 'cm-docs') : null;
                },
                label: item.label,
                type: COMPLETION_TYPES[item.kind ?? 0],
            }));
        return { from, options, validFor: /^[\w$]*$/ };
    };
}

function hover(client: LanguageClient, uri: string): Extension {
    return hoverTooltip(
        async (view, pos): Promise<Tooltip | null> => {
            const result = await client.request<{
                contents: Parameters<typeof markupText>[0];
                range?: LspRange;
            }>('textDocument/hover', {
                position: toLspPosition(view.state.doc, pos),
                textDocument: { uri },
            });
            const text = markupText(result?.contents);
            if (!result || !text) {
                return null;
            }
            const range = result.range
                ? fromLspRange(view.state.doc, result.range)
                : { from: pos, to: pos };
            return {
                above: true,
                create: () => ({
                    dom: markdownElement(text, 'cm-docs cm-docs-hover'),
                }),
                end: range.to,
                pos: range.from,
            };
        },
        { hoverTime: 300 },
    );
}

interface LspDiagnostic {
    range: LspRange;
    severity?: number;
    code?: number | string;
    source?: string;
    message: string;
    tags?: number[];
}

const SEVERITIES = ['error', 'error', 'warning', 'info', 'hint'] as const;

function diagnostics(client: LanguageClient, uri: string): Extension {
    return linter(
        async (view): Promise<Diagnostic[]> => {
            const result = await client.request<{ items?: LspDiagnostic[] }>(
                'textDocument/diagnostic',
                { textDocument: { uri } },
            );
            const doc = view.state.doc;
            return (result?.items ?? []).map((item) => {
                const { from, to } = fromLspRange(doc, item.range);
                return {
                    from,
                    message: item.message,
                    // An unused name fades rather than underlines.
                    severity: item.tags?.includes(1)
                        ? 'hint'
                        : SEVERITIES[item.severity ?? 1],
                    source: `${item.source ?? 'ts'}(${item.code ?? ''})`,
                    to: Math.max(to, Math.min(from + 1, doc.length)),
                };
            });
        },
        { delay: 120 },
    );
}

interface SignatureHelp {
    signatures: {
        label: string;
        documentation?: { value: string } | string;
        parameters?: {
            label: string | [number, number];
            documentation?: { value: string } | string;
        }[];
        activeParameter?: number;
    }[];
    activeSignature?: number;
    activeParameter?: number;
}

const setSignature = StateEffect.define<Tooltip | null>();

const signatureField = StateField.define<Tooltip | null>({
    create: () => null,
    provide: (field) => showTooltip.from(field),
    update(value, tr) {
        for (const effect of tr.effects) {
            if (effect.is(setSignature)) {
                return effect.value;
            }
        }
        return value;
    },
});

function signatureDom(help: SignatureHelp): HTMLElement {
    const signature = help.signatures[help.activeSignature ?? 0];
    const active = signature.activeParameter ?? help.activeParameter ?? 0;
    const parameter = signature.parameters?.[active];
    const dom = document.createElement('div');
    dom.className = 'cm-docs cm-signature';
    const label = document.createElement('div');
    label.className = 'cm-signature-label';
    let from = -1;
    let to = -1;
    if (Array.isArray(parameter?.label)) {
        [from, to] = parameter.label;
    } else if (typeof parameter?.label === 'string') {
        from = signature.label.indexOf(parameter.label);
        to = from + parameter.label.length;
    }
    if (from >= 0) {
        const strong = document.createElement('strong');
        strong.textContent = signature.label.slice(from, to);
        label.append(
            signature.label.slice(0, from),
            strong,
            signature.label.slice(to),
        );
    } else {
        label.textContent = signature.label;
    }
    dom.append(label);
    const docs = markupText(parameter?.documentation);
    if (docs) {
        dom.append(markdownElement(docs, 'cm-signature-docs'));
    }
    return dom;
}

/** Whether the cursor sits inside a call's parentheses on its own line. */
function insideCall(state: EditorState, pos: number): boolean {
    const line = state.doc.lineAt(pos);
    const before = state.sliceDoc(line.from, pos);
    let depth = 0;
    for (let i = before.length - 1; i >= 0; i--) {
        if (before[i] === ')') {
            depth++;
        } else if (before[i] === '(') {
            if (depth === 0) {
                return true;
            }
            depth--;
        }
    }
    return false;
}

function signatureHelp(client: LanguageClient, uri: string): Extension {
    const plugin = ViewPlugin.fromClass(
        class {
            private generation = 0;

            update(update: ViewUpdate) {
                if (!update.docChanged && !update.selectionSet) {
                    return;
                }
                const view = update.view;
                const pos = view.state.selection.main.head;
                const generation = ++this.generation;
                const shown = view.state.field(signatureField) !== null;
                if (!insideCall(view.state, pos)) {
                    if (shown) {
                        queueMicrotask(() =>
                            view.dispatch({ effects: setSignature.of(null) }),
                        );
                    }
                    return;
                }
                if (!update.docChanged && !shown) {
                    return;
                }
                void client
                    .request<SignatureHelp>('textDocument/signatureHelp', {
                        context: { isRetrigger: shown, triggerKind: 1 },
                        position: toLspPosition(view.state.doc, pos),
                        textDocument: { uri },
                    })
                    .then((help) => {
                        if (generation !== this.generation) {
                            return;
                        }
                        const tooltip: Tooltip | null =
                            help && help.signatures.length > 0
                                ? {
                                      above: true,
                                      create: () => ({
                                          dom: signatureDom(help),
                                      }),
                                      pos,
                                  }
                                : null;
                        view.dispatch({ effects: setSignature.of(tooltip) });
                    });
            }
        },
    );
    return [signatureField, plugin];
}

const setHighlights = StateEffect.define<DecorationSet>();
const highlightMark = Decoration.mark({ class: 'cm-occurrence' });

const highlightField = StateField.define<DecorationSet>({
    create: () => Decoration.none,
    provide: (field) => EditorView.decorations.from(field),
    update(value, tr) {
        for (const effect of tr.effects) {
            if (effect.is(setHighlights)) {
                return effect.value;
            }
        }
        return tr.docChanged ? Decoration.none : value;
    },
});

function occurrenceHighlights(client: LanguageClient, uri: string): Extension {
    const plugin = ViewPlugin.fromClass(
        class {
            private timer: ReturnType<typeof setTimeout> | null = null;

            update(update: ViewUpdate) {
                if (!update.selectionSet && !update.docChanged) {
                    return;
                }
                if (this.timer) {
                    clearTimeout(this.timer);
                }
                const view = update.view;
                this.timer = setTimeout(() => {
                    const state = view.state;
                    const pos = state.selection.main.head;
                    void client
                        .request<{ range: LspRange }[]>(
                            'textDocument/documentHighlight',
                            {
                                position: toLspPosition(state.doc, pos),
                                textDocument: { uri },
                            },
                        )
                        .then((ranges) => {
                            if (view.state.doc !== state.doc) {
                                return;
                            }
                            const marks = (ranges ?? [])
                                .map(({ range }) =>
                                    fromLspRange(state.doc, range),
                                )
                                .filter(({ from, to }) => to > from)
                                .sort((a, b) => a.from - b.from)
                                .map(({ from, to }) =>
                                    highlightMark.range(from, to),
                                );
                            view.dispatch({
                                effects: setHighlights.of(
                                    Decoration.set(marks),
                                ),
                            });
                        });
                }, 150);
            }

            destroy() {
                if (this.timer) {
                    clearTimeout(this.timer);
                }
            }
        },
    );
    return [highlightField, plugin];
}

/** Everything the language server shows on a patch buffer at `uri`. */
export function languageFeatures(
    client: LanguageClient,
    uri: string,
): Extension {
    return [
        EditorView.updateListener.of((update) => {
            if (update.docChanged) {
                client.change(uri, update.state.doc.toString());
            }
        }),
        autocompletion({
            activateOnTyping: true,
            icons: true,
            override: [completionSource(client, uri)],
        }),
        hover(client, uri),
        diagnostics(client, uri),
        signatureHelp(client, uri),
        occurrenceHighlights(client, uri),
    ];
}
