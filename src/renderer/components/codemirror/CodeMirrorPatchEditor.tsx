import { useEffect, useRef, useState } from 'react';
import {
    closeBrackets,
    closeBracketsKeymap,
    completionKeymap,
} from '@codemirror/autocomplete';
import {
    defaultKeymap,
    history,
    historyKeymap,
    indentWithTab,
} from '@codemirror/commands';
import { javascript } from '@codemirror/lang-javascript';
import { json } from '@codemirror/lang-json';
import {
    bracketMatching,
    indentOnInput,
    indentUnit,
} from '@codemirror/language';
import { lintKeymap } from '@codemirror/lint';
import {
    highlightSelectionMatches,
    search,
    searchKeymap,
} from '@codemirror/search';
import {
    Compartment,
    EditorState,
    type Extension,
    type StateEffect,
} from '@codemirror/state';
import {
    EditorView,
    crosshairCursor,
    drawSelection,
    highlightActiveLine,
    highlightActiveLineGutter,
    keymap,
    lineNumbers,
    rectangularSelection,
} from '@codemirror/view';
import type { LanguageServerInfo } from '../../../shared/ipcTypes';
import { fontFamilyWithFallback } from '../../../shared/fonts';
import type { Schemas } from '../../../shared/dsl/schemaTypeResolver';
import { apiIndexFor } from '../../dsl/apiSignatures';
import electronAPI from '../../electronAPI';
import { useTheme } from '../../themes/ThemeContext';
import type { PatchEditorProps } from '../MonacoPatchEditor';
import { createModelValueSync } from '../monaco/modelValueSync';
import { editorCodeFormat } from '../patchEditor/codeFormat';
import * as monaco from 'monaco-editor';
import { startModuleStatePolling } from '../monaco/moduleStateTracking';
import { BufferModel, setShownBufferModel } from './bufferModel';
import { LanguageClient } from './languageClient';
import { languageFeatures } from './languageFeatures';
import { navigation } from './navigation';
import { scopeZones, showScopes } from './scopeZones';
import { editorTheme } from './theme';
import './codemirror.css';

interface Buffer {
    state: EditorState;
    scroll: StateEffect<unknown> | null;
}

declare global {
    interface Window {
        /** The CodeMirror editor and its language client, for driving the app. */
        __CM__?: { view: EditorView; client: LanguageClient };
    }
}

const indentationOf = (tabSize: number): Extension => [
    EditorState.tabSize.of(tabSize),
    indentUnit.of(' '.repeat(tabSize)),
];

/**
 * Replaces the editor's text. `writing.current` is true for the duration, so
 * the change is recognised as the write and not as typing.
 */
function writeText(
    view: EditorView,
    text: string,
    writing: { current: boolean },
): void {
    writing.current = true;
    try {
        view.dispatch({
            changes: { from: 0, insert: text, to: view.state.doc.length },
        });
    } finally {
        writing.current = false;
    }
}

export function CodeMirrorPatchEditor({
    value,
    currentFile,
    openFiles,
    onChange,
    languageServer,
    scopeViews,
    scopeDecorations = null,
    onRegisterScopeCanvas,
    onUnregisterScopeCanvas,
    runningBufferId,
}: PatchEditorProps & { languageServer: LanguageServerInfo }) {
    const host = useRef<HTMLDivElement>(null);
    const [client] = useState(() => new LanguageClient(languageServer));
    const [view, setView] = useState<EditorView | null>(null);
    const [appearance] = useState(() => new Compartment());
    const [indentation] = useState(() => new Compartment());
    const buffers = useRef(new Map<string, Buffer>());
    const models = useRef(new Map<string, BufferModel>());
    const shown = useRef<string | undefined>(undefined);
    const [valueSync] = useState(createModelValueSync);
    const writing = useRef(false);
    const emit = useRef(onChange);
    useEffect(() => {
        emit.current = onChange;
    });
    const [lib] = useState(() => electronAPI.getDslLibSource());
    const dsl = useRef<{ libSource: string; schemas: Schemas } | null>(null);
    useEffect(() => {
        void Promise.all([lib, electronAPI.getSchemas()]).then(
            ([libSource, schemas]) => {
                dsl.current = { libSource, schemas };
            },
        );
    }, [lib]);

    const { theme, font, fontLigatures, fontSize, prettierConfig } = useTheme();
    const { tabSize } = editorCodeFormat(prettierConfig);
    const themeExtension = editorTheme(theme, {
        fontFamily: fontFamilyWithFallback(font),
        fontLigatures,
        fontSize,
    });
    const look = useRef({ prettierConfig, tabSize, themeExtension });
    useEffect(() => {
        look.current = { prettierConfig, tabSize, themeExtension };
    });

    useEffect(() => {
        if (!host.current) {
            return;
        }
        const created = new EditorView({ parent: host.current });
        window.__CM__ = { client, view: created };
        setView(created);
        return () => {
            created.destroy();
            delete window.__CM__;
            setView(null);
        };
    }, [client]);

    const createState = (bufferId: string, text: string): EditorState => {
        const isJson = bufferId.endsWith('.json');
        const uri = client.uriFor(bufferId);
        if (!isJson) {
            client.open(uri, text);
        }
        const model = new BufferModel();
        models.current.set(bufferId, model);
        const state = EditorState.create({
            doc: text,
            extensions: [
                lineNumbers(),
                history(),
                drawSelection(),
                highlightActiveLine(),
                highlightActiveLineGutter(),
                bracketMatching(),
                closeBrackets(),
                indentOnInput(),
                highlightSelectionMatches(),
                search(),
                rectangularSelection(),
                crosshairCursor(),
                EditorState.allowMultipleSelections.of(true),
                keymap.of([
                    ...closeBracketsKeymap,
                    ...defaultKeymap,
                    ...searchKeymap,
                    ...historyKeymap,
                    ...completionKeymap,
                    ...lintKeymap,
                    indentWithTab,
                ]),
                isJson ? json() : javascript(),
                isJson
                    ? []
                    : [
                          languageFeatures(client, uri),
                          navigation({
                              client,
                              controlApi: () =>
                                  dsl.current && {
                                      index: apiIndexFor(dsl.current.libSource),
                                      schemas: dsl.current.schemas,
                                  },
                              format: () => look.current.prettierConfig,
                              lib: () => lib,
                              look: () => look.current.themeExtension,
                              uri,
                          }),
                      ],
                model.extension(),
                scopeZones(),
                appearance.of(look.current.themeExtension),
                indentation.of(indentationOf(look.current.tabSize)),
                EditorView.updateListener.of((update) => {
                    if (!update.docChanged || writing.current) {
                        return;
                    }
                    const next = update.state.doc.toString();
                    valueSync.noteEmitted(next);
                    emit.current(next);
                }),
            ],
        });
        model.adopt(state);
        return state;
    };

    // One editor state per buffer, so each keeps its undo history, selection
    // and scroll while the editor shows another.
    useEffect(() => {
        if (!view || currentFile === undefined) {
            return;
        }
        if (shown.current !== currentFile) {
            if (shown.current !== undefined) {
                buffers.current.set(shown.current, {
                    scroll: view.scrollSnapshot(),
                    state: view.state,
                });
                models.current.get(shown.current)?.attach(null);
            }
            const buffer = buffers.current.get(currentFile);
            view.setState(buffer?.state ?? createState(currentFile, value));
            view.dispatch({
                effects: [
                    appearance.reconfigure(look.current.themeExtension),
                    indentation.reconfigure(
                        indentationOf(look.current.tabSize),
                    ),
                    ...(buffer?.scroll ? [buffer.scroll] : []),
                ],
            });
            shown.current = currentFile;
            const model = models.current.get(currentFile) ?? null;
            model?.attach(view);
            setShownBufferModel(model);
            valueSync.reset();
        }
        // `value` is written only when it is text the editor did not emit
        // (a reload from disk), so it never replaces newer keystrokes.
        const text = view.state.doc.toString();
        if (valueSync.shouldWrite(value, text)) {
            writeText(view, value, writing);
        }
    });

    useEffect(() => {
        view?.dispatch({
            effects: [
                appearance.reconfigure(themeExtension),
                indentation.reconfigure(indentationOf(tabSize)),
            ],
        });
    }, [view, appearance, indentation, themeExtension, tabSize]);

    useEffect(() => {
        const open = new Set(openFiles);
        for (const id of buffers.current.keys()) {
            if (!open.has(id)) {
                buffers.current.delete(id);
                models.current.get(id)?.dispose();
                models.current.delete(id);
                client.close(client.uriFor(id));
            }
        }
    }, [openFiles, client]);

    // After the effect that shows the buffer, so the panels go on its state.
    useEffect(() => {
        if (!view || currentFile === undefined) {
            return;
        }
        showScopes(
            view,
            (scopeViews ?? []).filter((scope) => scope.file === currentFile),
            scopeDecorations,
            {
                register: onRegisterScopeCanvas,
                unregister: onUnregisterScopeCanvas,
            },
        );
    }, [
        view,
        currentFile,
        scopeViews,
        scopeDecorations,
        onRegisterScopeCanvas,
        onUnregisterScopeCanvas,
    ]);

    // The steps the running patch is playing, highlighted on its buffer.
    useEffect(() => {
        if (!runningBufferId) {
            return;
        }
        return startModuleStatePolling({
            getModel: () =>
                (models.current.get(runningBufferId) ??
                    null) as unknown as monaco.editor.ITextModel | null,
            getModuleStates: () => electronAPI.synthesizer.getModuleStates(),
            monaco,
        });
    }, [runningBufferId]);

    return (
        <div className="patch-editor" style={{ height: '100%' }}>
            <div
                ref={host}
                style={{
                    display: currentFile ? 'block' : 'none',
                    height: '100%',
                }}
            />
        </div>
    );
}
