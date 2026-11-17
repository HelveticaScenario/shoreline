/**
 * Track labels for the editor's TypeScript worker. A label binds its track's
 * name, which JavaScript knows nothing of, so the worker reads each patch
 * with one declaration per track appended after the patch's own text:
 *
 *     var bass = __trackBinding(<the labelled expression>)
 *
 * The declaration sits in the patch's own module, so the name gets the type
 * of what it is bound to (the track's final signal, a send target when the
 * track is a bus) and the patch's
 * hover, completion and checking see it. A closing `export {}` keeps the
 * names in this patch even when it is read as a script. Every position in the
 * patch's text is untouched; whatever the language service reports past it
 * is dropped or moved onto the label's name, and a request made on a label's
 * name is asked of its declaration.
 */
import type * as TS from 'typescript';

import {
    analyzeTrackLabels,
    type TrackLabelAnalysis,
} from '../../../shared/dsl/trackLabels';

/** The track label diagnostic's code; outside TypeScript's own range. */
export const TRACK_LABEL_CODE = 90002;

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
    for (const { track, expression, labelStart } of analysis.tracks) {
        const head = `\n;var `;
        const declStart = text.length + head.length;
        const expressionText = source.slice(
            expression.getStart(file),
            expression.end,
        );
        text += `${head}${track.name} = __trackBinding(${expressionText});`;
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

/** Stands for a binding whose type the editor cannot tell. */
const UNKNOWN = '?';

/**
 * The lib type each track's name is bound as, by track name, or
 * {@link UNKNOWN}.
 */
function bindingTypes(
    ts: typeof TS,
    checker: TS.TypeChecker,
    sourceFile: TS.SourceFile,
    view: TrackLabelView,
): Map<string, string> {
    const starts = new Map(
        view.bindings.map((b, i) => [
            b.declStart,
            view.analysis.tracks[i].track.name,
        ]),
    );
    // Read off the labelled value, not the declared name: two tracks that
    // read each other leave both names untyped, while their values still
    // say what they are. An untyped value could be anything.
    const bound = (value: TS.Expression): string => {
        let type = checker.getTypeAtLocation(value);
        if ((type.flags & ts.TypeFlags.Any) !== 0) {
            return UNKNOWN;
        }
        if (type.getSymbol()?.name === 'Track') {
            [type] = checker.getTypeArguments(type as TS.TypeReference);
        }
        const name = type?.getSymbol()?.name;
        return name === 'GroupSignal' || name === 'RangedGroupSignal'
            ? 'GroupTap'
            : name === 'BusSignal' || name === 'RangedBusSignal'
              ? 'BusTap'
              : 'TrackTap';
    };
    const types = new Map<string, string>();
    for (const statement of sourceFile.statements) {
        if (!ts.isVariableStatement(statement)) {
            continue;
        }
        const [declaration] = statement.declarationList.declarations;
        const name = starts.get(declaration.name.getStart(sourceFile));
        if (name !== undefined) {
            const [value] = ts.isCallExpression(declaration.initializer!)
                ? declaration.initializer.arguments
                : [];
            types.set(name, value ? bound(value) : UNKNOWN);
        }
    }
    return types;
}

/**
 * One diagnostic per label whose suffix names a track that is not a group,
 * and per bus track given a group, on the suffix's name.
 */
export function groupSuffixDiagnostics(
    ts: typeof TS,
    checker: TS.TypeChecker,
    sourceFile: TS.SourceFile,
    view: TrackLabelView,
): TS.Diagnostic[] {
    if (view.bindings.length === 0) {
        return [];
    }
    const types = bindingTypes(ts, checker, sourceFile, view);
    const diagnostics: TS.Diagnostic[] = [];
    for (const { track, labelStart, labelEnd } of view.analysis.tracks) {
        if (track.group === null || !types.has(track.group)) {
            continue;
        }
        const target = types.get(track.group);
        const message =
            types.get(track.name) === 'BusTap'
                ? `${track.name} is a bus, and a bus cannot play into a group`
                : target === 'GroupTap' || target === UNKNOWN
                  ? null
                  : `${track.group} is not a group. A group is a track whose signal is $group() or methods chained on it`;
        if (message !== null) {
            const start = labelStart + track.groupStart;
            diagnostics.push({
                category: ts.DiagnosticCategory.Error,
                code: TRACK_LABEL_CODE,
                file: sourceFile,
                length: labelEnd - start,
                messageText: message,
                start,
            });
        }
    }
    return diagnostics;
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

/** One diagnostic per bus track whose label solos it, on the label's prefix. */
export function busSoloDiagnostics(
    ts: typeof TS,
    checker: TS.TypeChecker,
    sourceFile: TS.SourceFile,
    view: TrackLabelView,
): TS.Diagnostic[] {
    if (view.bindings.length === 0) {
        return [];
    }
    const types = bindingTypes(ts, checker, sourceFile, view);
    return view.analysis.tracks.flatMap(
        ({ track, labelStart }): TS.Diagnostic[] =>
            track.state.solo && types.get(track.name) === 'BusTap'
                ? [
                      {
                          category: ts.DiagnosticCategory.Error,
                          code: TRACK_LABEL_CODE,
                          file: sourceFile,
                          length: track.prefixLength,
                          messageText: `${track.name} is a bus, and a bus cannot be soloed`,
                          start: labelStart,
                      },
                  ]
                : [],
    );
}
