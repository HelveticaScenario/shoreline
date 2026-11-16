/**
 * Which bus a signal is rooted at: what `$bus()` returns, and every signal
 * chained from one (`$bus().plate().wet(2)`). A track labelling such a signal
 * is that bus's track, the one sends reach by its name. A signal that merely
 * reads a bus, as a module argument or in a mix, is rooted at none.
 */
const ROOTS = new WeakMap<object, object>();

export function setBusRoot(signal: object, bus: object): void {
    ROOTS.set(signal, bus);
}

export function busRootOf(signal: unknown): object | undefined {
    return typeof signal === 'object' && signal !== null
        ? ROOTS.get(signal)
        : undefined;
}

/** Root `result` at the bus `receiver` is rooted at, if any. */
export function carryBusRoot(result: unknown, receiver: unknown): void {
    const bus = busRootOf(receiver);
    if (bus && typeof result === 'object' && result !== null) {
        ROOTS.set(result, bus);
    }
}
