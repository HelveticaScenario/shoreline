/**
 * Inlay hints naming the output a module root stands for at each place it is
 * used: `$saw(seq‹._cv›).amp($perc(seq‹._trig›))`.
 *
 * Sites the type checker resolves show as the code is typed. Sites it cannot
 * (an untyped helper parameter, say) are filled from the resolutions the
 * runtime recorded the last time the buffer ran, for as long as the value's
 * text is unchanged.
 */

import type {
    editor,
    IDisposable,
    languages,
    Position,
    Range,
} from 'monaco-editor';
import type { OutputResolutionRecord } from '../../../shared/dsl/outputResolutionRule';
import {
    siteOfRecord,
    type SignalSite,
    type SignalSiteTables,
} from '../../../shared/dsl/signalSites';
import electronAPI from '../../electronAPI';
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { DslTypeScriptWorkerApi } from './dslTsWorker';

/** A site the runtime resolved, its value tracked through edits by a decoration. */
interface RuntimeFill {
    decorationId: string;
    /** The value's text and slot when the buffer ran. */
    text: string;
    slot: string;
    outputs: string[];
    insertable: boolean;
}

/** The provider's state, set once it is registered. */
let active: {
    monaco: Monaco;
    tables: Promise<SignalSiteTables>;
    /** Makes Monaco request the hints afresh. */
    refresh: () => void;
} | null = null;

const runtimeFills = new WeakMap<editor.ITextModel, RuntimeFill[]>();

async function signalSites(model: editor.ITextModel): Promise<SignalSite[]> {
    const { monaco, tables } = active!;
    const getWorker = await monaco.typescript.getJavaScriptWorker();
    const worker = (await getWorker(
        model.uri,
    )) as unknown as DslTypeScriptWorkerApi;
    return worker.getSignalSites(model.uri.toString(), await tables);
}

function rangeOf(model: editor.ITextModel, site: SignalSite): Range {
    const start = model.getPositionAt(site.start);
    const end = model.getPositionAt(site.end);
    return new active!.monaco.Range(
        start.lineNumber,
        start.column,
        end.lineNumber,
        end.column,
    );
}

/**
 * Attach the resolutions a run of `source` recorded to the sites the type
 * checker left unresolved. Nothing changes when `model` no longer reads as
 * `source`: a record is only as good as the text it was made from.
 */
export async function applyRuntimeResolutions(
    model: editor.ITextModel,
    source: string,
    records: readonly OutputResolutionRecord[],
): Promise<void> {
    if (!active || model.getValue() !== source) {
        return;
    }
    const sites = await signalSites(model);
    if (model.isDisposed() || model.getValue() !== source) {
        return;
    }

    const fills = new Map<SignalSite, Omit<RuntimeFill, 'decorationId'>>();
    for (const record of records) {
        const site = siteOfRecord(sites, record);
        if (!site || site.output !== null) {
            continue;
        }
        const fill = fills.get(site) ?? {
            insertable: site.insertable,
            outputs: [],
            slot: site.slot,
            text: model.getValueInRange(rangeOf(model, site)),
        };
        if (!fill.outputs.includes(record.output)) {
            fill.outputs.push(record.output);
        }
        // An element's record names what the element stands for, which
        // cannot be written after the array it sits in.
        fill.insertable &&= record.slot === site.slot;
        fills.set(site, fill);
    }

    const ids = model.deltaDecorations(
        (runtimeFills.get(model) ?? []).map((fill) => fill.decorationId),
        [...fills.keys()].map((site) => ({
            options: {
                stickiness:
                    active!.monaco.editor.TrackedRangeStickiness
                        .NeverGrowsWhenTypingAtEdges,
            },
            range: rangeOf(model, site),
        })),
    );
    runtimeFills.set(
        model,
        [...fills.values()].map((fill, i) => ({
            ...fill,
            decorationId: ids[i],
        })),
    );
    active.refresh();
}

function hint(
    position: Position,
    outputs: readonly string[],
    insertable: boolean,
): languages.InlayHint {
    const label = outputs.map((output) => `._${output}`).join(' | ');
    return {
        label,
        position,
        // Accepting the hint writes the output into the code, where one
        // output can be written after the value.
        textEdits:
            outputs.length === 1 && insertable
                ? [
                      {
                          range: active!.monaco.Range.fromPositions(position),
                          text: label,
                      },
                  ]
                : undefined,
    };
}

/** Register the hints for JavaScript models. */
export function registerOutputResolutionHints(monaco: Monaco): IDisposable {
    const provider: languages.InlayHintsProvider = {
        async provideInlayHints(model, range, token) {
            const sites = await signalSites(model);
            if (token.isCancellationRequested || model.isDisposed()) {
                return { dispose: () => {}, hints: [] };
            }
            const hints: languages.InlayHint[] = [];
            const unresolved = new Map<number, SignalSite>();
            for (const site of sites) {
                if (site.output === null) {
                    unresolved.set(site.end, site);
                } else {
                    hints.push(
                        hint(
                            model.getPositionAt(site.end),
                            [site.output],
                            site.insertable,
                        ),
                    );
                }
            }
            // A fill stands only while its value reads as it did when run and
            // is still an unresolved site of the same slot.
            for (const fill of runtimeFills.get(model) ?? []) {
                const tracked = model.getDecorationRange(fill.decorationId);
                if (!tracked || model.getValueInRange(tracked) !== fill.text) {
                    continue;
                }
                const end = tracked.getEndPosition();
                if (
                    unresolved.get(model.getOffsetAt(end))?.slot === fill.slot
                ) {
                    hints.push(hint(end, fill.outputs, fill.insertable));
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
    active = {
        monaco,
        refresh: () => {
            registration.dispose();
            registration = register();
        },
        tables: electronAPI.getSignalSiteTables(),
    };

    return {
        dispose() {
            registration.dispose();
            active = null;
        },
    };
}
