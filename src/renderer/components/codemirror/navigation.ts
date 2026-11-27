/**
 * Commands that ask the language server about the name under the cursor: go
 * to and peek its definition, peek its references, rename it, and list the
 * fixes on offer there.
 */
import { type Extension, StateEffect, StateField } from '@codemirror/state';
import {
    EditorView,
    type KeyBinding,
    type Tooltip,
    keymap,
    showTooltip,
} from '@codemirror/view';
import type { PrettierConfig } from '../../../shared/ipcTypes';
import type { ControlApi } from '../../dsl/controlQuickFix';
import { computeControlQuickFixes } from '../../dsl/controlQuickFix';
import { formatControlQuickFix } from '../../dsl/controlFixFormat';
import { codeStyleFromPrettier } from '../../dsl/listEdits';
import { formatDsl } from '../monaco/formattingProvider';
import {
    type LanguageClient,
    type LspLocation,
    type LspRange,
    type LspWorkspaceEdit,
    changesFor,
    fromLspPosition,
    fromLspRange,
    toLspPosition,
} from './languageClient';
import { type PeekLocation, peekPanel, showPeek } from './peek';

export interface NavigationContext {
    client: LanguageClient;
    uri: string;
    /** The editor's look, which a peek's preview shares. */
    look: () => Extension;
    /** The text of the DSL's declarations. */
    lib: () => Promise<string>;
    /** What resolves a control quick fix; null until the schemas load. */
    controlApi: () => ControlApi | null;
    format: () => PrettierConfig;
}

const cursor = (view: EditorView) => view.state.selection.main.head;

function request<T>(view: EditorView, ctx: NavigationContext, method: string) {
    return ctx.client.request<T>(method, {
        context: { includeDeclaration: true },
        position: toLspPosition(view.state.doc, cursor(view)),
        textDocument: { uri: ctx.uri },
    });
}

function select(view: EditorView, from: number, to: number): void {
    view.dispatch({
        effects: EditorView.scrollIntoView(from, { y: 'center' }),
        selection: { anchor: from, head: to },
    });
    view.focus();
}

/** The text of a 0-based line, trimmed, for a peek's list. */
function lineLabel(text: string, range: LspRange): string {
    const line = text.split('\n')[range.start.line] ?? '';
    return `${range.start.line + 1}: ${line.trim()}`;
}

async function peekLocations(
    view: EditorView,
    ctx: NavigationContext,
    locations: LspLocation[],
): Promise<PeekLocation[]> {
    const doc = view.state.doc;
    const own = doc.toString();
    const lib = locations.some((l) => l.uri !== ctx.uri) ? await ctx.lib() : '';
    return locations.map((location): PeekLocation => {
        if (location.uri === ctx.uri) {
            const { from, to } = fromLspRange(doc, location.range);
            return {
                from,
                jump: () => select(view, from, to),
                label: lineLabel(own, location.range),
                text: own,
                to,
            };
        }
        const lines = lib.split('\n');
        const offset = (p: { line: number; character: number }) =>
            lines.slice(0, p.line).reduce((n, l) => n + l.length + 1, 0) +
            p.character;
        return {
            from: offset(location.range.start),
            label: lineLabel(lib, location.range),
            text: lib,
            to: offset(location.range.end),
        };
    });
}

async function definitions(view: EditorView, ctx: NavigationContext) {
    const result = await request<LspLocation | LspLocation[]>(
        view,
        ctx,
        'textDocument/definition',
    );
    return result ? (Array.isArray(result) ? result : [result]) : [];
}

function showPeekAt(
    view: EditorView,
    ctx: NavigationContext,
    title: string,
    locations: PeekLocation[],
): void {
    if (locations.length > 0) {
        view.dispatch({
            effects: showPeek.of({
                locations,
                look: ctx.look(),
                pos: cursor(view),
                title,
            }),
        });
    }
}

/** Go to the definition, or peek it when it is outside the buffer. */
export function goToDefinition(ctx: NavigationContext) {
    return (view: EditorView): boolean => {
        void definitions(view, ctx).then(async (found) => {
            const [first] = found;
            if (!first) {
                return;
            }
            if (first.uri === ctx.uri) {
                const { from, to } = fromLspRange(view.state.doc, first.range);
                select(view, from, to);
                return;
            }
            showPeekAt(
                view,
                ctx,
                'Definition',
                await peekLocations(view, ctx, found),
            );
        });
        return true;
    };
}

export function peekDefinition(ctx: NavigationContext) {
    return (view: EditorView): boolean => {
        void definitions(view, ctx).then(async (found) =>
            showPeekAt(
                view,
                ctx,
                'Definition',
                await peekLocations(view, ctx, found),
            ),
        );
        return true;
    };
}

export function peekReferences(ctx: NavigationContext) {
    return (view: EditorView): boolean => {
        void request<LspLocation[]>(view, ctx, 'textDocument/references').then(
            async (found) => {
                const locations = (found ?? [])
                    .filter((l) => l.uri === ctx.uri)
                    .sort((a, b) => a.range.start.line - b.range.start.line);
                showPeekAt(
                    view,
                    ctx,
                    `${locations.length} references`,
                    await peekLocations(view, ctx, locations),
                );
            },
        );
        return true;
    };
}

const setFloating = StateEffect.define<Tooltip | null>();

/** The one rename box, fix menu or message open at the cursor. */
const floatingField = StateField.define<Tooltip | null>({
    create: () => null,
    provide: (field) => showTooltip.from(field),
    update(value, tr) {
        for (const effect of tr.effects) {
            if (effect.is(setFloating)) {
                return effect.value;
            }
        }
        return tr.docChanged || tr.selection ? null : value;
    },
});

function message(view: EditorView, pos: number, text: string): void {
    view.dispatch({
        effects: setFloating.of({
            above: true,
            create() {
                const dom = document.createElement('div');
                dom.className = 'cm-message';
                dom.textContent = text;
                return { dom };
            },
            pos,
        }),
    });
}

export function rename(ctx: NavigationContext) {
    return (view: EditorView): boolean => {
        const pos = cursor(view);
        void request<{ range: LspRange; placeholder?: string } | LspRange>(
            view,
            ctx,
            'textDocument/prepareRename',
        ).then((prepared) => {
            if (!prepared) {
                message(view, pos, 'You cannot rename this element.');
                return;
            }
            const range = 'range' in prepared ? prepared.range : prepared;
            const { from, to } = fromLspRange(view.state.doc, range);
            const current = view.state.sliceDoc(from, to);
            view.dispatch({
                effects: setFloating.of({
                    create() {
                        const dom = document.createElement('div');
                        dom.className = 'cm-rename';
                        const input = dom.appendChild(
                            document.createElement('input'),
                        );
                        input.value = current;
                        input.spellcheck = false;
                        input.size = Math.max(current.length + 2, 12);
                        input.onkeydown = (event) => {
                            event.stopPropagation();
                            if (event.key === 'Escape') {
                                view.dispatch({
                                    effects: setFloating.of(null),
                                });
                                view.focus();
                            } else if (event.key === 'Enter') {
                                void applyRename(view, ctx, pos, input.value);
                            }
                        };
                        return {
                            dom,
                            mount: () => {
                                input.focus();
                                input.select();
                            },
                        };
                    },
                    pos: from,
                }),
            });
        });
        return true;
    };
}

async function applyRename(
    view: EditorView,
    ctx: NavigationContext,
    pos: number,
    newName: string,
): Promise<void> {
    const edit = await ctx.client.request<LspWorkspaceEdit>(
        'textDocument/rename',
        {
            newName,
            position: toLspPosition(view.state.doc, pos),
            textDocument: { uri: ctx.uri },
        },
    );
    const changes = changesFor(view.state.doc, edit, ctx.uri);
    if (changes.length === 0) {
        message(view, pos, 'Only a name the patch declares can be renamed');
        return;
    }
    view.dispatch({ changes, effects: setFloating.of(null) });
    view.focus();
}

interface Fix {
    title: string;
    preferred?: boolean;
    run: () => void | Promise<void>;
}

interface LspCodeAction {
    title: string;
    kind?: string;
    isPreferred?: boolean;
    edit?: LspWorkspaceEdit;
}

async function fixesAt(
    view: EditorView,
    ctx: NavigationContext,
): Promise<Fix[]> {
    const { state } = view;
    const { from, to } = state.selection.main;
    const source = state.doc.toString();
    const fixes: Fix[] = [];

    const api = ctx.controlApi();
    if (api) {
        const format = ctx.format();
        for (const fix of computeControlQuickFixes(
            source,
            from,
            api,
            codeStyleFromPrettier(format),
        )) {
            fixes.push({
                preferred: fix.preferred,
                async run() {
                    const formatted = await formatControlQuickFix(
                        source,
                        fix,
                        format,
                    );
                    view.dispatch({
                        changes: {
                            from: formatted.span.start,
                            insert: formatted.newText,
                            to: formatted.span.end,
                        },
                        selection: {
                            anchor:
                                formatted.span.start + formatted.caretOffset,
                        },
                    });
                },
                title: fix.title,
            });
        }
    }

    const range = {
        end: toLspPosition(state.doc, to),
        start: toLspPosition(state.doc, from),
    };
    const pulled = await ctx.client.request<{
        items?: { range: LspRange }[];
    }>('textDocument/diagnostic', { textDocument: { uri: ctx.uri } });
    const actions = await ctx.client.request<LspCodeAction[]>(
        'textDocument/codeAction',
        {
            context: {
                diagnostics: (pulled?.items ?? []).filter((d) => {
                    const start = fromLspPosition(state.doc, d.range.start);
                    const end = fromLspPosition(state.doc, d.range.end);
                    return start <= to && end >= from;
                }),
                triggerKind: 1,
            },
            range,
            textDocument: { uri: ctx.uri },
        },
    );
    for (const action of actions ?? []) {
        const changes = changesFor(state.doc, action.edit ?? null, ctx.uri);
        if (changes.length > 0) {
            fixes.push({
                preferred: action.isPreferred,
                run: () => view.dispatch({ changes }),
                title: action.title,
            });
        }
    }
    return fixes;
}

function fixMenu(view: EditorView, pos: number, fixes: Fix[]): Tooltip {
    return {
        create() {
            const dom = document.createElement('ul');
            dom.className = 'cm-fix-menu';
            dom.tabIndex = -1;
            let selected = Math.max(
                0,
                fixes.findIndex((fix) => fix.preferred),
            );
            const close = () => {
                view.dispatch({ effects: setFloating.of(null) });
                view.focus();
            };
            const apply = (fix: Fix) => {
                close();
                void fix.run();
            };
            const items = fixes.map((fix, index) => {
                const item = dom.appendChild(document.createElement('li'));
                item.textContent = fix.title;
                item.onmousedown = (event) => {
                    event.preventDefault();
                    apply(fix);
                };
                item.onmouseenter = () => mark(index);
                return item;
            });
            const mark = (index: number) => {
                selected = (index + fixes.length) % fixes.length;
                items.forEach((item, i) =>
                    item.classList.toggle('selected', i === selected),
                );
            };
            mark(selected);
            dom.onkeydown = (event) => {
                event.stopPropagation();
                event.preventDefault();
                if (event.key === 'ArrowDown') {
                    mark(selected + 1);
                } else if (event.key === 'ArrowUp') {
                    mark(selected - 1);
                } else if (event.key === 'Enter') {
                    apply(fixes[selected]);
                } else if (event.key === 'Escape') {
                    close();
                }
            };
            return { dom, mount: () => dom.focus() };
        },
        pos,
    };
}

export function quickFix(ctx: NavigationContext) {
    return (view: EditorView): boolean => {
        const pos = cursor(view);
        void fixesAt(view, ctx).then((fixes) => {
            if (fixes.length === 0) {
                message(view, pos, 'No code actions available');
                return;
            }
            view.dispatch({
                effects: setFloating.of(fixMenu(view, pos, fixes)),
            });
        });
        return true;
    };
}

export function formatDocument(ctx: NavigationContext) {
    return (view: EditorView): boolean => {
        const source = view.state.doc.toString();
        void formatDsl(source, ctx.format()).then((formatted) => {
            if (formatted !== source && view.state.doc.toString() === source) {
                view.dispatch({
                    changes: { from: 0, insert: formatted, to: source.length },
                    selection: {
                        anchor: Math.min(cursor(view), formatted.length),
                    },
                });
            }
        });
        return true;
    };
}

export function navigation(ctx: NavigationContext): Extension {
    const bindings: KeyBinding[] = [
        { key: 'F12', run: goToDefinition(ctx) },
        { key: 'Alt-F12', run: peekDefinition(ctx) },
        { key: 'Shift-F12', run: peekReferences(ctx) },
        { key: 'F2', run: rename(ctx) },
        { key: 'Mod-.', run: quickFix(ctx) },
        { key: 'Shift-Alt-f', run: formatDocument(ctx) },
    ];
    return [
        floatingField,
        peekPanel(),
        keymap.of(bindings),
        EditorView.domEventHandlers({
            mousedown(event, view) {
                if (!(event.metaKey || event.ctrlKey) || event.button !== 0) {
                    return false;
                }
                const pos = view.posAtCoords({
                    x: event.clientX,
                    y: event.clientY,
                });
                if (pos === null) {
                    return false;
                }
                view.dispatch({ selection: { anchor: pos } });
                peekDefinition(ctx)(view);
                return true;
            },
        }),
    ];
}
