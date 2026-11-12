/**
 * Patterns as the DSL runtime handles them. A patch script holds a handle
 * that answers as the `$cycle` playing the pattern; the engine takes the wire
 * payload behind it.
 */

import {
    captureSourceLocation,
    withSourceLocation,
} from './captureSourceLocation';

/**
 * The wire payload behind each pattern a patch script holds. The script's
 * value is a proxy that answers as the pattern's `$cycle` and keeps the
 * payload's fields out of reach, so host code reads them through here.
 */
const PAYLOADS = new WeakMap<object, object>();

/**
 * Every pattern payload, with where the script built it when that is known.
 */
const BUILT_AT = new WeakMap<
    object,
    { line: number; column: number } | undefined
>();

/**
 * Record `payload` as the wire payload of the pattern `handle`, built by the
 * running script call.
 */
export function registerPattern(handle: object, payload: object): void {
    PAYLOADS.set(handle, payload);
    BUILT_AT.set(payload, captureSourceLocation());
}

/** Types a script's handle by the wire payload behind it. */
export declare const PAYLOAD: unique symbol;

type PayloadOf<T> = T extends { readonly [PAYLOAD]: infer P } ? P : T;

/** The wire payload of a pattern: itself unless it is a script's handle. */
export function patternPayload<T>(pattern: T): PayloadOf<T> {
    return (
        typeof pattern === 'object' && pattern !== null
            ? (PAYLOADS.get(pattern) ?? pattern)
            : pattern
    ) as PayloadOf<T>;
}

/** Whether `value` is a pattern: a script's handle or a wire payload. */
export function isPattern(value: unknown): value is object {
    return (
        typeof value === 'object' &&
        value !== null &&
        BUILT_AT.has(patternPayload(value))
    );
}

type CycleFactory = (pattern: unknown, ...rest: unknown[]) => unknown;

/**
 * The `$cycle` factory of the active DSL execution, which plays every
 * pattern. Set by executor.ts before running user code, cleared after.
 */
let activeCycleFactory: CycleFactory | null = null;

/** The `$cycle` each pattern plays through in the active DSL execution. */
let sharedCycles = new WeakMap<object, unknown>();

export function setActiveCycleFactory(factory: CycleFactory | null): void {
    activeCycleFactory = factory;
    sharedCycles = new WeakMap();
}

/** A new `$cycle` playing `pattern`, given `$cycle`'s remaining arguments. */
export function makeCycle(pattern: object, rest: unknown[]): unknown {
    if (!activeCycleFactory) {
        throw new Error('a pattern can only be played while a patch runs');
    }
    return activeCycleFactory(pattern, ...rest);
}

/**
 * The one `$cycle` a pattern plays through when it is used as a module: read
 * as a signal, or through any collection member. Made on first use, as a
 * module of the call that built the pattern, so its errors point there.
 */
export function cycleOf(pattern: object): unknown {
    const payload = patternPayload(pattern);
    let cycle = sharedCycles.get(payload);
    if (cycle === undefined) {
        cycle = withSourceLocation(BUILT_AT.get(payload), () =>
            makeCycle(payload, []),
        );
        sharedCycles.set(payload, cycle);
    }
    return cycle;
}

/** `value` as a signal: a pattern is its `$cycle`, anything else itself. */
export function patternSignal(value: unknown): unknown {
    return isPattern(value) ? cycleOf(value) : value;
}
