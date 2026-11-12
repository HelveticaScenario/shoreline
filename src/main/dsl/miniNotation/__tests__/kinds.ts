import type { PatternPayload } from '../index';

/** Whether `value` is the wire payload of a pattern of kind `kind`. */
export function isKind<K extends PatternPayload['__kind']>(
    value: unknown,
    kind: K,
): value is Extract<PatternPayload, { __kind: K }> {
    return (
        typeof value === 'object' &&
        value !== null &&
        (value as { __kind?: unknown }).__kind === kind
    );
}
