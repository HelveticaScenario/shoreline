/**
 * Call Site Span Analyzer
 *
 * Tracks the full source span (start line through end line) of DSL method
 * calls like .scope() and $v.preview() and standalone calls like $slider(). This information
 * is used to position view zones after the closing paren of multi-line calls.
 *
 * Designed to be extensible — any DSL function whose view zone needs to know
 * its full call expression span can be added to the tracking sets.
 */
import ts from 'typescript';

import {
    callSiteKey,
    lineColumn,
    v8CallAnchor,
} from '../../shared/dsl/ast/position';
import { forEachDescendant } from '../../shared/dsl/ast/walk';
import type { CallSiteSpanRegistry } from './sourceAnalysisTypes';

/** Method names tracked for call expression spans (property access calls) */
const DSL_METHODS_TO_TRACK = new Set([
    'scope',
    'out',
    'outMono',
    'eq',
    'preview',
]);

/** Standalone function names tracked for call expression spans */
const DSL_FUNCTIONS_TO_TRACK = new Set([
    '$slider',
    '$btn',
    '$toggleBtn',
    '$eq',
]);

function isTracked(call: ts.CallExpression): boolean {
    const callee = call.expression;
    if (ts.isPropertyAccessExpression(callee)) {
        return DSL_METHODS_TO_TRACK.has(callee.name.text);
    }
    return ts.isIdentifier(callee) && DSL_FUNCTIONS_TO_TRACK.has(callee.text);
}

/**
 * The start and end lines of every tracked call in `file`, keyed in
 * patch-source coordinates so the renderer can look one up directly from a
 * `captureSourceLocation()` result.
 */
export function analyzeCallSiteSpans(
    file: ts.SourceFile,
): CallSiteSpanRegistry {
    const callSiteSpans: CallSiteSpanRegistry = new Map();
    forEachDescendant(file, (node) => {
        if (!ts.isCallExpression(node) || !isTracked(node)) {
            return;
        }
        const anchor = v8CallAnchor(node, file);
        callSiteSpans.set(callSiteKey(file, anchor), {
            // The line holding the call's last character, its closing paren.
            endLine: lineColumn(file, node.getEnd() - 1).line,
            startLine: lineColumn(file, anchor).line,
        });
    });
    return callSiteSpans;
}
