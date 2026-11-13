/**
 * Ghost `.mono()` hints after each value that reaches an input folding it to
 * mono while it may be stereo: `$enosc(pitch‹.mono()›)`. Accepting one writes
 * the fold into the code, which reads the same.
 *
 * Only a run can tell what is stereo, so the hints come from the folds the
 * runtime recorded the last time the buffer ran, each standing for as long
 * as its value's text is unchanged.
 */

import type { editor, IDisposable, IRange, languages } from 'monaco-editor';
import type { FoldRecord } from '../../../shared/dsl/outputResolutionRule';
import {
    siteOfRecord,
    type SignalSiteTables,
} from '../../../shared/dsl/signalSites';
import electronAPI from '../../electronAPI';
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { DslTypeScriptWorkerApi } from './dslTsWorker';

const LABEL = '.mono()';

/** A folding value, its text tracked through edits by a decoration. */
interface Fold {
    decorationId: string;
    /** The value's text when the buffer ran. */
    text: string;
    /** Whether `.mono()` can be written straight after the value. */
    insertable: boolean;
}

let active: {
    monaco: Monaco;
    tables: Promise<SignalSiteTables>;
    /** Makes Monaco request the hints afresh. */
    refresh: () => void;
} | null = null;

const folds = new WeakMap<editor.ITextModel, Fold[]>();

/**
 * Show the folds a run of `source` recorded. Nothing changes when `model` no
 * longer reads as `source`: a record is only as good as the text it was made
 * from.
 */
export async function applyFolds(
    model: editor.ITextModel,
    source: string,
    records: readonly FoldRecord[],
): Promise<void> {
    if (!active || model.getValue() !== source) {
        return;
    }
    const { monaco, tables } = active;
    const getWorker = await monaco.typescript.getJavaScriptWorker();
    const worker = (await getWorker(
        model.uri,
    )) as unknown as DslTypeScriptWorkerApi;
    const sites = await worker.getValueSites(
        model.uri.toString(),
        await tables,
    );
    if (!active || model.isDisposed() || model.getValue() !== source) {
        return;
    }

    // A record for an element of an array not written as a literal finds
    // the whole array, which `.mono()` cannot follow.
    const found = new Map<string, { range: IRange; fold: boolean }>();
    for (const record of records) {
        const site = siteOfRecord(sites, record);
        if (!site) {
            continue;
        }
        const start = model.getPositionAt(site.start);
        const end = model.getPositionAt(site.end);
        found.set(`${site.start}:${site.end}`, {
            fold: site.insertable && site.slot === record.slot,
            range: new monaco.Range(
                start.lineNumber,
                start.column,
                end.lineNumber,
                end.column,
            ),
        });
    }

    const entries = [...found.values()];
    const ids = model.deltaDecorations(
        (folds.get(model) ?? []).map((fold) => fold.decorationId),
        entries.map(({ range }) => ({
            options: {
                stickiness:
                    monaco.editor.TrackedRangeStickiness
                        .NeverGrowsWhenTypingAtEdges,
            },
            range,
        })),
    );
    folds.set(
        model,
        entries.map(({ range, fold }, i) => ({
            decorationId: ids[i],
            insertable: fold,
            text: model.getValueInRange(range),
        })),
    );
    active.refresh();
}

/** Register the hints for JavaScript models. */
export function registerFoldHints(monaco: Monaco): IDisposable {
    const provider: languages.InlayHintsProvider = {
        provideInlayHints(model, range) {
            const hints: languages.InlayHint[] = [];
            for (const fold of folds.get(model) ?? []) {
                const tracked = model.getDecorationRange(fold.decorationId);
                if (!tracked || model.getValueInRange(tracked) !== fold.text) {
                    continue;
                }
                const position = tracked.getEndPosition();
                if (!range.containsPosition(position)) {
                    continue;
                }
                hints.push({
                    label: LABEL,
                    position,
                    textEdits: fold.insertable
                        ? [
                              {
                                  range: monaco.Range.fromPositions(position),
                                  text: LABEL,
                              },
                          ]
                        : undefined,
                    tooltip: 'Folds a stereo signal to mono here',
                });
            }
            return { dispose: () => {}, hints };
        },
    };

    // Monaco stops listening to a provider's `onDidChangeInlayHints` after
    // its second request, so new folds are announced by registering afresh,
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
