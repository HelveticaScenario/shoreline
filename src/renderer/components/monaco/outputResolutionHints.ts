/**
 * Inlay hints naming the output a module root stands for where it is used,
 * and folding what may be stereo where an input reads it folded:
 * `$saw(seq‹._cv›)`, `$enosc(pair‹.mono()›)`, `$coinFlip(pair)‹._heads.mono()›.scope()`.
 *
 * All show as the code is typed. An output the type checker cannot name (an
 * untyped helper parameter, say) comes from the runtime's last run, while the
 * value's text is unchanged. So does whether a file a module plays is mono,
 * while the call's text is unchanged: a fold through mono files alone is no
 * fold.
 */

import type {
    editor,
    IDisposable,
    languages,
    Position,
    Range,
} from 'monaco-editor';
import type { ModuleChannelsRecord } from '../../../shared/dsl/channels';
import type { OutputResolutionRecord } from '../../../shared/dsl/outputResolutionRule';
import {
    siteOfRecord,
    type SignalSite,
    type SignalSiteTables,
} from '../../../shared/dsl/signalSites';
import electronAPI from '../../electronAPI';
import type { Monaco } from '../../hooks/useCustomMonaco';
import { dslWorker } from './dslWorker';
import { callKey, foldsGiven, monoFileCalls } from './monoFileCalls';

/** A site the runtime resolved, its value tracked through edits by a decoration. */
interface RuntimeFill {
    decorationId: string;
    /** The value's text and slot when the buffer ran. */
    text: string;
    slot: string;
    outputs: string[];
    insertable: boolean;
}

/** A file call every run of which built a mono module, tracked through edits by a decoration. */
interface MonoCall {
    decorationId: string;
    /** The call's text and module when the buffer ran. */
    text: string;
    module: string;
}

/** The provider's state, set once it is registered. */
let active: {
    monaco: Monaco;
    tables: Promise<SignalSiteTables>;
    /** Makes Monaco request the hints afresh. */
    refresh: () => void;
} | null = null;

const runtimeFills = new WeakMap<editor.ITextModel, RuntimeFill[]>();
const monoCalls = new WeakMap<editor.ITextModel, MonoCall[]>();

async function signalSites(model: editor.ITextModel): Promise<SignalSite[]> {
    const { monaco, tables } = active!;
    const worker = await dslWorker(monaco, model.uri);
    return worker.getSignalSites(model.uri.toString(), await tables);
}

function offsetRange(
    model: editor.ITextModel,
    span: { start: number; end: number },
): Range {
    const start = model.getPositionAt(span.start);
    const end = model.getPositionAt(span.end);
    return new active!.monaco.Range(
        start.lineNumber,
        start.column,
        end.lineNumber,
        end.column,
    );
}

/**
 * Attach the resolutions a run of `source` recorded to the sites the type
 * checker left unresolved, and note the file calls it found mono. Nothing
 * changes when `model` no longer reads as `source`: a record is only as good
 * as the text it was made from.
 */
export async function applyRuntimeResolutions(
    model: editor.ITextModel,
    source: string,
    records: readonly OutputResolutionRecord[],
    moduleChannels: readonly ModuleChannelsRecord[],
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
            text: model.getValueInRange(offsetRange(model, site)),
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
            range: offsetRange(model, site),
        })),
    );
    runtimeFills.set(
        model,
        [...fills.values()].map((fill, i) => ({
            ...fill,
            decorationId: ids[i],
        })),
    );

    const mono = monoFileCalls(sites, moduleChannels);
    const monoIds = model.deltaDecorations(
        (monoCalls.get(model) ?? []).map((call) => call.decorationId),
        mono.map((call) => ({
            options: {
                stickiness:
                    active!.monaco.editor.TrackedRangeStickiness
                        .NeverGrowsWhenTypingAtEdges,
            },
            range: offsetRange(model, call),
        })),
    );
    monoCalls.set(
        model,
        mono.map((call, i) => ({
            decorationId: monoIds[i],
            module: call.module,
            text: model.getValueInRange(offsetRange(model, call)),
        })),
    );
    active.refresh();
}

function hint(
    position: Position,
    outputs: readonly string[],
    insertable: boolean,
    fold: boolean,
): languages.InlayHint {
    const mono = fold ? '.mono()' : '';
    const label =
        outputs.length === 0
            ? mono
            : outputs.map((output) => `._${output}${mono}`).join(' | ');
    return {
        label,
        position,
        tooltip: fold ? 'Folds a stereo signal to mono here' : undefined,
        // Accepting the hint writes it into the code, where at most one
        // output can be written after the value.
        textEdits:
            outputs.length <= 1 && insertable
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
            // A mono call stands only while it reads as it did when run.
            const mono = new Set<string>();
            for (const call of monoCalls.get(model) ?? []) {
                const tracked = model.getDecorationRange(call.decorationId);
                if (tracked && model.getValueInRange(tracked) === call.text) {
                    mono.add(
                        callKey(
                            model.getOffsetAt(tracked.getStartPosition()),
                            model.getOffsetAt(tracked.getEndPosition()),
                            call.module,
                        ),
                    );
                }
            }
            const hints: languages.InlayHint[] = [];
            const unresolved = new Map<number, SignalSite>();
            for (const site of sites) {
                const fold = foldsGiven(site, mono);
                if (site.output === null) {
                    unresolved.set(site.end, site);
                } else if (site.output !== undefined || fold) {
                    hints.push(
                        hint(
                            model.getPositionAt(site.end),
                            site.output === undefined ? [] : [site.output],
                            site.insertable,
                            fold,
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
                const site = unresolved.get(model.getOffsetAt(end));
                if (site?.slot === fill.slot) {
                    unresolved.delete(site.end);
                    hints.push(
                        hint(
                            end,
                            fill.outputs,
                            fill.insertable,
                            foldsGiven(site, mono),
                        ),
                    );
                }
            }
            // An unfilled root that folds still folds.
            for (const site of unresolved.values()) {
                if (foldsGiven(site, mono)) {
                    hints.push(
                        hint(model.getPositionAt(site.end), [], false, true),
                    );
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
