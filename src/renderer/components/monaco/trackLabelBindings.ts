/**
 * Track labels for the editor's TypeScript worker. A label binds its track's
 * name, which JavaScript knows nothing of, so the worker reads each patch
 * with one declaration per track appended after the patch's own text:
 *
 *     function __tracks(bass, lead) {
 *         return { bass: (<bass's expression>), lead: (<lead's expression>) };
 *     }
 *     var bass = __trackBinding(__tracks().bass)
 *
 * The declarations sit in the patch's own module, so each name gets the type
 * of what it is bound to (the track's final signal, a send target when the
 * track is a bus) and the patch's hover, completion and checking see it.
 * Inside `__tracks` every track name is a parameter typed as a plain track,
 * so no name's type waits on another's and tracks may read each other. A
 * closing `export {}` keeps the names in this patch even when it is read as a
 * script. Every position in the patch's text is untouched; whatever the
 * language service reports past it is dropped or moved onto the label's
 * name, and a request made on a label's name is asked of its declaration.
 */
import type * as TS from 'typescript';

import {
    analyzeTrackLabels,
    type TrackLabelAnalysis,
} from '../../../shared/dsl/trackLabels';

/** The track label diagnostic's code; outside TypeScript's own range. */
export const TRACK_LABEL_CODE = 90002;

/** The appended function whose result holds every track's value. */
const TRACKS = '__tracks';

/**
 * Whether a completion entry is one of the editor's own names, which a patch
 * never writes: the appended function, or a `__` name the DSL's types declare.
 */
export function isEditorInternal(entry: {
    name: string;
    kindModifiers?: string;
}): boolean {
    return (
        entry.name === TRACKS ||
        (entry.name.startsWith('__') &&
            (entry.kindModifiers ?? '').split(',').includes('declare'))
    );
}

/** One appended declaration and the label name it stands for. */
interface Binding {
    /** Span of the declared name in the appended text. */
    declStart: number;
    declEnd: number;
    /** Span of the name inside the label (without prefix or suffix). */
    nameStart: number;
    nameEnd: number;
}

export interface TrackLabelView {
    /** The patch's own text length; offsets past it are appended text. */
    length: number;
    /** The text the language service reads: the patch, then its bindings. */
    text: string;
    analysis: TrackLabelAnalysis;
    bindings: Binding[];
    /**
     * The patch's own parser errors at its end, which the appended text moves
     * past it in the language service's reading.
     */
    endDiagnostics: readonly TS.DiagnosticWithLocation[];
}

/** The language service's view of `source`. */
export function trackLabelView(ts: typeof TS, source: string): TrackLabelView {
    const file = ts.createSourceFile(
        'patch.js',
        source,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.JS,
    );
    const analysis = analyzeTrackLabels(ts, file);
    const parseDiagnostics = (
        file as unknown as { parseDiagnostics: TS.DiagnosticWithLocation[] }
    ).parseDiagnostics;
    const endDiagnostics = parseDiagnostics.filter(
        (d) => d.start >= source.length,
    );
    let text = source;
    const bindings: Binding[] = [];
    const { tracks } = analysis;
    if (tracks.length > 0) {
        const parameters = tracks
            .map(({ track }) => `/** @type {TrackTap} */ ${track.name}`)
            .join(', ');
        const values = tracks
            .map(({ track, expression }) => {
                const value = source.slice(
                    expression.getStart(file),
                    expression.end,
                );
                return `${track.name}: (${value})`;
            })
            .join(',\n');
        text += `\n;function ${TRACKS}(${parameters}) { return {\n${values}\n}; }`;
    }
    for (const { track, labelStart } of tracks) {
        const head = `\n;var `;
        const declStart = text.length + head.length;
        text += `${head}${track.name} = __trackBinding(${TRACKS}().${track.name});`;
        const nameStart = labelStart + track.prefixLength;
        bindings.push({
            declEnd: declStart + track.name.length,
            declStart,
            nameEnd: nameStart + track.name.length,
            nameStart,
        });
    }
    if (bindings.length === 0) {
        return {
            analysis,
            bindings,
            endDiagnostics,
            length: source.length,
            text,
        };
    }
    text += '\n;export {};';
    // A construct the patch leaves open (a block, a string) swallows the
    // appended text, which then binds nothing and would only add errors.
    if (parseDiagnostics.length > 0) {
        const viewFile = ts.createSourceFile(
            'patch.js',
            text,
            ts.ScriptTarget.Latest,
            false,
            ts.ScriptKind.JS,
        );
        const declared = new Set(
            viewFile.statements
                .filter(ts.isVariableStatement)
                .map((st) => st.declarationList.declarations[0]?.name.pos),
        );
        if (!bindings.every((b) => declared.has(b.declStart - 1))) {
            return {
                analysis,
                bindings: [],
                endDiagnostics,
                length: source.length,
                text: source,
            };
        }
    }
    return { analysis, bindings, endDiagnostics, length: source.length, text };
}

/** `position` moved from a label's name onto its declaration. */
export function bindingPosition(
    view: TrackLabelView,
    position: number,
): number {
    const binding = view.bindings.find(
        (b) => position >= b.nameStart && position <= b.nameEnd,
    );
    return binding
        ? binding.declStart + position - binding.nameStart
        : position;
}

/**
 * `position` moved from a label's mute/solo prefix to the start of its name,
 * so a rename asked anywhere before the name's end is the name's.
 */
export function renamePosition(view: TrackLabelView, position: number): number {
    const index = view.analysis.tracks.findIndex(
        (t) => position >= t.labelStart && position <= t.labelEnd,
    );
    const nameStart = view.bindings[index]?.nameStart ?? position;
    return Math.max(position, nameStart);
}

/** Whether a rename location starting at `start` is a track's declared name. */
export function isBindingStart(view: TrackLabelView, start: number): boolean {
    return view.bindings.some((b) => b.declStart === start);
}

/** Whether `position` is on a track label but not on its name. */
export function onLabelOutsideName(
    view: TrackLabelView,
    position: number,
): boolean {
    return view.analysis.tracks.some(
        (t, i) =>
            position >= t.labelStart &&
            position <= t.labelEnd &&
            !(
                position >= (view.bindings[i]?.nameStart ?? -1) &&
                position <= (view.bindings[i]?.nameEnd ?? -1)
            ),
    );
}

/** `span` moved from an appended declaration onto its label, or null. */
export function mapSpan(
    view: TrackLabelView,
    span: TS.TextSpan,
): TS.TextSpan | null {
    if (span.start < view.length) {
        return span;
    }
    const binding = view.bindings.find(
        (b) => span.start >= b.declStart && span.start < b.declEnd,
    );
    return binding
        ? {
              length: binding.nameEnd - binding.nameStart,
              start: binding.nameStart,
          }
        : null;
}

/** The binding whose declared name `start` falls in. */
function bindingAt(view: TrackLabelView, start: number): Binding | undefined {
    return view.bindings.find((b) => start >= b.declStart && start < b.declEnd);
}

/**
 * Diagnostics on the patch's own text. One on a declared name means the label
 * cannot bind it, and moves onto the label's name; the rest past the patch
 * are dropped.
 */
export function withinPatchDiagnostics(
    view: TrackLabelView,
    diagnostics: readonly TS.Diagnostic[],
): TS.Diagnostic[] {
    return diagnostics.flatMap((d): TS.Diagnostic[] => {
        if (d.start === undefined || d.start < view.length) {
            return [d];
        }
        const binding = bindingAt(view, d.start);
        if (!binding) {
            return [];
        }
        const name = view.text.slice(binding.declStart, binding.declEnd);
        return [
            {
                ...d,
                length: binding.nameEnd - binding.nameStart,
                messageText: `${name} is already taken, so this track label cannot bind it`,
                relatedInformation: undefined,
                start: binding.nameStart,
            },
        ];
    });
}

/**
 * Parser diagnostics on the patch's own text: those the language service
 * reports, the patch's own at its end, and one per broken label.
 */
export function withTrackLabelDiagnostics(
    ts: typeof TS,
    sourceFile: TS.SourceFile,
    view: TrackLabelView,
    diagnostics: readonly TS.Diagnostic[],
): TS.Diagnostic[] {
    const kept = diagnostics.filter(
        (d) => d.start === undefined || d.start < view.length,
    );
    // Without appended text the language service reports these itself.
    const appended = view.text.length > view.length;
    const atEnd = (appended ? view.endDiagnostics : []).map(
        (d): TS.Diagnostic => ({ ...d, file: sourceFile }),
    );
    const added = view.analysis.errors.map(
        (e): TS.Diagnostic => ({
            category: ts.DiagnosticCategory.Error,
            code: TRACK_LABEL_CODE,
            file: sourceFile,
            length: e.end - e.start,
            messageText: e.message,
            start: e.start,
        }),
    );
    return [...kept, ...atEnd, ...added].sort(
        (a, b) => (a.start ?? 0) - (b.start ?? 0),
    );
}

/** Items on the patch's own text only. */
export function withinPatch<T extends { start?: number }>(
    view: TrackLabelView,
    items: readonly T[],
): T[] {
    return items.filter((d) => d.start === undefined || d.start < view.length);
}

/** Each program's global value names, declared by the libs. */
const GLOBAL_NAMES = new WeakMap<TS.Program, Set<string>>();

function globalNames(ts: typeof TS, program: TS.Program): Set<string> {
    let names = GLOBAL_NAMES.get(program);
    if (!names) {
        names = new Set();
        const lib = program
            .getSourceFiles()
            .find((f) => f.isDeclarationFile && !ts.isExternalModule(f));
        const symbols = lib
            ? program
                  .getTypeChecker()
                  .getSymbolsInScope(lib, ts.SymbolFlags.Value)
            : [];
        for (const symbol of symbols) {
            if (
                symbol.declarations?.every(
                    (d) => d.getSourceFile().isDeclarationFile,
                )
            ) {
                names.add(symbol.name);
            }
        }
        GLOBAL_NAMES.set(program, names);
    }
    return names;
}

/**
 * One diagnostic per label whose name a global already has, on the name. The
 * appended declaration sits in a module, where it would shadow the global
 * without complaint, while the run cannot bind such a name.
 */
export function globalNameDiagnostics(
    ts: typeof TS,
    program: TS.Program,
    sourceFile: TS.SourceFile,
    view: TrackLabelView,
): TS.Diagnostic[] {
    if (view.bindings.length === 0) {
        return [];
    }
    const globals = globalNames(ts, program);
    return view.analysis.tracks.flatMap(({ track }, i): TS.Diagnostic[] => {
        if (!globals.has(track.name)) {
            return [];
        }
        const { nameStart, nameEnd } = view.bindings[i];
        return [
            {
                category: ts.DiagnosticCategory.Error,
                code: TRACK_LABEL_CODE,
                file: sourceFile,
                length: nameEnd - nameStart,
                messageText: `${track.name} is already taken, so this track label cannot bind it`,
                start: nameStart,
            },
        ];
    });
}
