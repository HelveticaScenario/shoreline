/**
 * Inlay hints naming the output a module root stands for at each place it is
 * used: `$saw(seq‹.cv›).amp($perc(seq‹.trig›))`.
 *
 * Sites the type checker resolves show as the code is typed. Sites it cannot
 * (an untyped helper parameter, say) are filled from the resolutions the
 * runtime recorded the last time the buffer ran, for as long as the value's
 * text is unchanged.
 */

import type { editor, IDisposable, languages } from 'monaco-editor';
import type { OutputResolutionRecord } from '../../../shared/dsl/outputResolutionRule';
import {
    siteOfRecord,
    type SignalSite,
    type SignalSiteTables,
} from '../../../shared/dsl/signalSites';
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { DslTypeScriptWorkerApi } from './dslTsWorker';

/** A site the runtime resolved, tracked through edits by a decoration. */
interface RuntimeFill {
    decorationId: string;
    /** The value's text when the buffer ran. */
    text: string;
    outputs: string[];
}

const runtimeFills = new WeakMap<editor.ITextModel, RuntimeFill[]>();

/** Called when a model's runtime fills change, to make Monaco re-request hints. */
const fillListeners = new Set<() => void>();

/** Set once the provider is registered; a run before that has no hints to fill. */
let active: { monaco: Monaco; tables: Promise<SignalSiteTables> } | null = null;

async function signalSites(
    monaco: Monaco,
    model: editor.ITextModel,
    tables: SignalSiteTables,
): Promise<SignalSite[]> {
    const getWorker = await monaco.typescript.getJavaScriptWorker();
    const worker = (await getWorker(
        model.uri,
    )) as unknown as DslTypeScriptWorkerApi;
    return worker.getSignalSites(model.uri.toString(), tables);
}

/** The run of identifier characters ending at `offset`: the value's tail. */
function tailRange(
    monaco: Monaco,
    model: editor.ITextModel,
    offset: number,
): { range: import('monaco-editor').Range; text: string } {
    const end = model.getPositionAt(offset);
    const line = model.getLineContent(end.lineNumber).slice(0, end.column - 1);
    const text = line.match(/[\w$]+$|.$/)?.[0] ?? '';
    return {
        range: new monaco.Range(
            end.lineNumber,
            end.column - text.length,
            end.lineNumber,
            end.column,
        ),
        text,
    };
}

/**
 * Attach the resolutions recorded by a run of `model`'s current text to the
 * sites the type checker left unresolved.
 */
export async function applyRuntimeResolutions(
    model: editor.ITextModel,
    records: readonly OutputResolutionRecord[],
): Promise<void> {
    if (!active) {
        return;
    }
    const { monaco } = active;
    const version = model.getVersionId();
    const sites = await signalSites(monaco, model, await active.tables);
    if (model.isDisposed() || model.getVersionId() !== version) {
        return;
    }

    const outputsBySite = new Map<SignalSite, string[]>();
    for (const record of records) {
        const site = siteOfRecord(sites, record);
        if (!site || site.output !== null) {
            continue;
        }
        const outputs = outputsBySite.get(site) ?? [];
        if (!outputs.includes(record.output)) {
            outputs.push(record.output);
        }
        outputsBySite.set(site, outputs);
    }

    const entries = [...outputsBySite].map(([site, outputs]) => ({
        outputs,
        ...tailRange(monaco, model, site.anchor),
    }));
    const previous = runtimeFills.get(model) ?? [];
    const ids = model.deltaDecorations(
        previous.map((fill) => fill.decorationId),
        entries.map(({ range }) => ({
            options: {
                stickiness:
                    monaco.editor.TrackedRangeStickiness
                        .NeverGrowsWhenTypingAtEdges,
            },
            range,
        })),
    );
    runtimeFills.set(
        model,
        entries.map(({ outputs, text }, i) => ({
            decorationId: ids[i],
            outputs,
            text,
        })),
    );
    for (const listener of fillListeners) {
        listener();
    }
}

function hint(
    monaco: Monaco,
    model: editor.ITextModel,
    offset: number,
    outputs: readonly string[],
): languages.InlayHint {
    const position = model.getPositionAt(offset);
    const label = outputs.map((output) => `.${output}`).join(' | ');
    return {
        label,
        position,
        // Accepting the hint writes the output into the code. A site that
        // resolved several ways has no single text to write.
        textEdits:
            outputs.length === 1
                ? [
                      {
                          range: monaco.Range.fromPositions(position),
                          text: label,
                      },
                  ]
                : undefined,
    };
}

/**
 * Register the hints for JavaScript models. `loadTables` is called once, on
 * first use.
 */
export function registerOutputResolutionHints(
    monaco: Monaco,
    loadTables: () => Promise<SignalSiteTables>,
): IDisposable {
    active ??= { monaco, tables: loadTables() };
    const { tables } = active;

    const provider: languages.InlayHintsProvider = {
        async provideInlayHints(model, range, token) {
            const sites = await signalSites(monaco, model, await tables);
            if (token.isCancellationRequested || model.isDisposed()) {
                return { dispose: () => {}, hints: [] };
            }
            const hints: languages.InlayHint[] = [];
            const unresolved = new Set<number>();
            for (const site of sites) {
                if (site.output === null) {
                    unresolved.add(site.anchor);
                } else {
                    hints.push(hint(monaco, model, site.anchor, [site.output]));
                }
            }
            // A fill stands only while its value reads as it did when run
            // and the type checker still cannot resolve that site.
            for (const fill of runtimeFills.get(model) ?? []) {
                const tracked = model.getDecorationRange(fill.decorationId);
                if (!tracked || model.getValueInRange(tracked) !== fill.text) {
                    continue;
                }
                const offset = model.getOffsetAt(tracked.getEndPosition());
                if (unresolved.has(offset)) {
                    hints.push(hint(monaco, model, offset, fill.outputs));
                }
            }
            return {
                dispose: () => {},
                hints: hints.filter((h) => range.containsPosition(h.position)),
            };
        },
    };

    // Monaco stops listening to a provider's `onDidChangeInlayHints` after
    // its second request, so new fills are announced by registering afresh,
    // which always starts a new request.
    const register = () =>
        monaco.languages.registerInlayHintsProvider('javascript', provider);
    let registration = register();
    const refresh = () => {
        registration.dispose();
        registration = register();
    };
    fillListeners.add(refresh);

    return {
        dispose() {
            fillListeners.delete(refresh);
            registration.dispose();
        },
    };
}
