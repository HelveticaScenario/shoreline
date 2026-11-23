/**
 * Line offset for DSL code wrapper.
 * The executePatchScript creates a script body with 'use strict' which adds
 * lines before user code.  This offset is configured by executor.ts at
 * runtime via `setDSLWrapperLineOffset`.
 */
let dslWrapperLineOffset = 2;

export function setDSLWrapperLineOffset(offset: number): void {
    dslWrapperLineOffset = offset;
}

type SourceLocation = { line: number; column: number };

let pinnedLocation: SourceLocation | undefined;

/**
 * Run `body` with every capture inside it reading `location`: a call made on
 * the script's behalf belongs to the place the script wrote, not to the place
 * that happened to trigger it.
 */
export function withSourceLocation<T>(
    location: SourceLocation | undefined,
    body: () => T,
): T {
    if (!location) {
        return body();
    }
    const outer = pinnedLocation;
    pinnedLocation = location;
    try {
        return body();
    } finally {
        pinnedLocation = outer;
    }
}

/**
 * Capture source location from the current stack trace.
 * Looks for the `<anonymous>` frame which corresponds to DSL code executed
 * in the vm script compiled by `executePatchScript` (the default vm script
 * filename is `evalmachine.<anonymous>`).
 *
 * Returns `undefined` if the source location cannot be determined.
 */
export function captureSourceLocation(): SourceLocation | undefined {
    if (pinnedLocation) {
        return pinnedLocation;
    }
    const stackHolder: { stack?: string } = {};
    Error.captureStackTrace(stackHolder, captureSourceLocation);

    if (!stackHolder.stack) {
        return undefined;
    }

    const location = scriptLocationIn(stackHolder.stack);
    return location?.column === undefined
        ? undefined
        : { column: location.column, line: location.line };
}

/**
 * Where in the patch source an error thrown while compiling or running the
 * patch script points: a SyntaxError's stack opens with the script's line,
 * and any other error's innermost script frame is the call the user wrote.
 */
export function errorSourceLocation(
    error: unknown,
): { line: number; column?: number } | undefined {
    const stack =
        typeof error === 'object' && error !== null
            ? (error as { stack?: unknown }).stack
            : undefined;
    return typeof stack === 'string' ? scriptLocationIn(stack) : undefined;
}

/** ` (line N)` for an error message about `location`, or nothing without one. */
export function lineSuffix(location: { line: number } | undefined): string {
    return location ? ` (line ${location.line})` : '';
}

/**
 * The first patch-source position in a stack: frames of the vm script
 * compiled by `executePatchScript` read `evalmachine.<anonymous>:line:column`,
 * and a SyntaxError's header gives only the line.
 */
function scriptLocationIn(
    stack: string,
): { line: number; column?: number } | undefined {
    for (const match of stack.matchAll(/<anonymous>:(\d+)(?::(\d+))?/g)) {
        const line = parseInt(match[1], 10) - dslWrapperLineOffset;
        if (line > 0) {
            return {
                column:
                    match[2] === undefined ? undefined : parseInt(match[2], 10),
                line,
            };
        }
    }
    return undefined;
}
