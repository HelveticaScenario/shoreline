/**
 * Track labels for the editor's TypeScript worker. A label binds its track's
 * name, which JavaScript knows nothing of, so the worker reads each patch
 * with one declaration per track appended after the patch's own text:
 *
 *     var bass = __trackBinding(<the labelled expression>)
 *
 * The declaration sits in the patch's own module, so the name gets the type
 * of what it is bound to (a bus, or the track's final signal) and the patch's
 * hover, completion and checking see it. Every position in the patch's text
 * is untouched; whatever the language service reports past it is dropped or
 * moved onto the label's name.
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
    let text = source;
    const bindings: Binding[] = [];
    // Text appended to a patch that does not parse would join its broken
    // construct and carry the parser's errors past the patch's end.
    const parses =
        (file as unknown as { parseDiagnostics: unknown[] }).parseDiagnostics
            .length === 0;
    for (const { track, expression, labelStart } of parses
        ? analysis.tracks
        : []) {
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
    return { analysis, bindings, length: source.length, text };
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

/** Diagnostics on the patch's own text, with one per broken label. */
export function withTrackLabelDiagnostics(
    ts: typeof TS,
    sourceFile: TS.SourceFile,
    view: TrackLabelView,
    diagnostics: readonly TS.Diagnostic[],
): TS.Diagnostic[] {
    const kept = diagnostics.filter(
        (d) => d.start === undefined || d.start < view.length,
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
    return [...kept, ...added].sort((a, b) => (a.start ?? 0) - (b.start ?? 0));
}

/** Diagnostics on the patch's own text only. */
export function withinPatch<T extends { start?: number }>(
    view: TrackLabelView,
    items: readonly T[],
): T[] {
    return items.filter((d) => d.start === undefined || d.start < view.length);
}
