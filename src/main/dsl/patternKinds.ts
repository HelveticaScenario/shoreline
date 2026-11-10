/**
 * Wire-shape discriminators of every pattern value: `$p(...)`,
 * `$p.s(...)`, `$p.arrange(...)` and the `.fast`/`.slow`/`.struct`/`.beat`/
 * `.glide` chains built from them.
 */
export const PATTERN_KINDS: ReadonlySet<unknown> = new Set([
    'ParsedPattern',
    'SpPattern',
    'ArrangePattern',
    'FastPattern',
    'SlowPattern',
    'StructPattern',
    'BeatPattern',
    'GlidePattern',
]);

export interface PatternValue {
    readonly __kind: string;
}

export function isPatternValue(value: unknown): value is PatternValue {
    return (
        typeof value === 'object' &&
        value !== null &&
        PATTERN_KINDS.has((value as { __kind?: unknown }).__kind)
    );
}

/**
 * Throw when `value` is a pattern where a signal belongs. `what` names the
 * place (`$sine: freq`). A pattern becomes a signal by playing it.
 */
export function refusePattern(value: unknown, what: string): void {
    if (isPatternValue(value)) {
        throw new Error(
            `${what} is a pattern, not a signal — play it with .c or .cycle()`,
        );
    }
}
