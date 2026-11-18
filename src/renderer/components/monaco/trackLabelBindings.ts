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

/** The span of the group name in the label suffix `position` is on. */
export function groupSuffixAt(
    view: TrackLabelView,
    position: number,
): TS.TextSpan | null {
    for (const { track, labelStart } of view.analysis.tracks) {
        const start = labelStart + track.groupStart;
        if (
            track.group !== null &&
            position >= start &&
            position <= start + track.group.length
        ) {
            return { length: track.group.length, start };
        }
    }
    return null;
}

/** The binding of the track named `name`. */
function bindingNamed(view: TrackLabelView, name: string): Binding | undefined {
    return view.bindings[
        view.analysis.tracks.findIndex((t) => t.track.name === name)
    ];
}

/**
 * `position` moved onto the name it reads in a label: from a group suffix to
 * the group's own label, where that group is a track.
 */
function namePosition(view: TrackLabelView, position: number): number {
    const suffix = groupSuffixAt(view, position);
    const group =
        suffix &&
        bindingNamed(
            view,
            view.text.slice(suffix.start, suffix.start + suffix.length),
        );
    return suffix && group
        ? group.nameStart + position - suffix.start
        : position;
}

/**
 * `position` moved from a label's name, or from a suffix naming its group,
 * onto the declaration of that name.
 */
export function bindingPosition(
    view: TrackLabelView,
    position: number,
): number {
    const at = namePosition(view, position);
    const binding = view.bindings.find(
        (b) => at >= b.nameStart && at <= b.nameEnd,
    );
    return binding ? binding.declStart + at - binding.nameStart : position;
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

/**
 * The group suffixes naming the track whose label name starts at `nameStart`:
 * every place a label reads that name besides its own.
 */
export function groupSuffixesOf(
    view: TrackLabelView,
    nameStart: number,
): TS.TextSpan[] {
    const index = view.bindings.findIndex((b) => b.nameStart === nameStart);
    const name = view.analysis.tracks[index]?.track.name;
    return name === undefined
        ? []
        : view.analysis.tracks.flatMap(({ track, labelStart }) =>
              track.group === name
                  ? [
                        {
                            length: name.length,
                            start: labelStart + track.groupStart,
                        },
                    ]
                  : [],
          );
}

/**
 * Whether `position` is on a track label but on neither its name nor a group
 * name the patch binds.
 */
export function onLabelOutsideName(
    view: TrackLabelView,
    position: number,
): boolean {
    return (
        namePosition(view, position) === position &&
        view.analysis.tracks.some(
            (t, i) =>
                position >= t.labelStart &&
                position <= t.labelEnd &&
                !(
                    position >= (view.bindings[i]?.nameStart ?? -1) &&
                    position <= (view.bindings[i]?.nameEnd ?? -1)
                ),
        )
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
    // The labelled value says what its track is. An untyped value could be
    // anything.
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
