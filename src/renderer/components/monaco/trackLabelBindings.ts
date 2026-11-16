/**
 * Track labels for the editor's TypeScript worker. A label binds its track's
 * name, which JavaScript knows nothing of, so the worker reads each patch
 * with one declaration per track appended after the patch's own text:
 *
 *     var bass = __trackBinding(<the labelled expression>)
 *
 * The declaration sits in the patch's own module, so the name gets the type
 * of what it is bound to (a bus, or the track's final signal) and the patch's
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
