/**
 * Source Analysis Orchestration
 *
 * Thin entry point that parses the patch once and runs the argument span
 * analyzer and call site span analyzer over the same tree.
 */

import type { ModuleSchema } from '@modular/core';

import { parseDsl } from '../../shared/dsl/ast/parse';
import type { AnalysisResult } from './sourceAnalysisTypes';
import { analyzeArgumentSpans } from './argumentSpanAnalyzer';
import { analyzeCallSiteSpans } from './callSiteSpanAnalyzer';

// Re-export types and utilities so existing consumers can import from here
export type {
    SourceSpan,
    ResolvedInterpolation,
    InterpolationResolutionMap,
    CallSiteSpans,
    CallSiteKey,
    SpanRegistry,
    CallExpressionSpan,
    CallSiteSpanRegistry,
    AnalysisResult,
} from './sourceAnalysisTypes';
export {
    setActiveInterpolationResolutions,
    getActiveInterpolationResolutions,
} from './sourceAnalysisTypes';

/**
 * Analyze DSL source code and build registries for argument locations
 * and call expression spans.
 *
 * @param source - The DSL source code to analyze
 * @param schemas - Module schemas to determine which calls to track
 * @param lineOffset - Lines the executor's script wrapper puts before the
 *   patch, added to argument registry keys
 * @returns Analysis result with span registry, interpolation resolution map,
 *          and call site span registry
 */
export function analyzeSourceSpans(
    source: string,
    schemas: ModuleSchema[],
    lineOffset: number = 0,
): AnalysisResult {
    const file = parseDsl(source);
    const { registry, interpolationResolutions } = analyzeArgumentSpans(
        file,
        schemas,
        lineOffset,
    );
    const callSiteSpans = analyzeCallSiteSpans(file);
    return { callSiteSpans, interpolationResolutions, registry };
}
