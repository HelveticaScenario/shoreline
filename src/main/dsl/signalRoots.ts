/**
 * Which bus or group a signal is rooted at: what `$bus()` or `$group()`
 * returns, and every signal chained from one (`$bus().plate().wet(2)`). A
 * track labelling such a signal is that bus's or group's track, the one
 * sends or label suffixes reach by its name. A signal that merely reads one,
 * as a module argument or in a mix, is rooted at none, and so is a module's
 * set of several outputs and any one picked from it.
 */
const ROOTS = new WeakMap<object, object>();

export function setSignalRoot(signal: object, root: object): void {
    ROOTS.set(signal, root);
}

export function signalRootOf(signal: unknown): object | undefined {
    return typeof signal === 'object' && signal !== null
        ? ROOTS.get(signal)
        : undefined;
}

/** Root `result` where `receiver` is rooted, if anywhere. */
export function carrySignalRoot(result: unknown, receiver: unknown): void {
    const root = signalRootOf(receiver);
    if (root && typeof result === 'object' && result !== null) {
        ROOTS.set(result, root);
    }
}
