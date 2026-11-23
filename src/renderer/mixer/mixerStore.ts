import { useSyncExternalStore } from 'react';
import type { VuMeterDef, VuMeterGhost } from '../../shared/dsl/vuMeterTypes';

export interface MixerSnapshot {
    /** The running patch's meters, in the order the patch declares them. */
    outputs: VuMeterDef[];
    /** Code-side control values diverging from the running audio after
     *  code-only (Ctrl/Cmd) edits, keyed by meter key. */
    ghosts: ReadonlyMap<string, VuMeterGhost>;
}

/**
 * The mixer's state. Gesture handlers and the meter loop read it
 * synchronously, so a change is visible to a redraw or a second gesture in
 * the same tick; React subscribes through {@link useMixerSnapshot}.
 */
export class MixerStore {
    private snapshot: MixerSnapshot = { ghosts: new Map(), outputs: [] };
    private readonly listeners = new Set<() => void>();

    readonly subscribe = (listener: () => void): (() => void) => {
        this.listeners.add(listener);
        return () => this.listeners.delete(listener);
    };

    readonly getSnapshot = (): MixerSnapshot => this.snapshot;

    get outputs(): VuMeterDef[] {
        return this.snapshot.outputs;
    }

    output(key: string): VuMeterDef | undefined {
        return this.snapshot.outputs.find((o) => o.key === key);
    }

    ghost(key: string): VuMeterGhost | undefined {
        return this.snapshot.ghosts.get(key);
    }

    setOutputs(outputs: VuMeterDef[]): void {
        this.set({ ...this.snapshot, outputs });
    }

    /** Replace the output named `key` with `update` of it. */
    updateOutput(
        key: string,
        update: (output: VuMeterDef) => VuMeterDef,
    ): void {
        this.setOutputs(
            this.snapshot.outputs.map((o) => (o.key === key ? update(o) : o)),
        );
    }

    /** Show an applied patch's meters. The patch was compiled from the
     *  source, so audio and code agree and every ghost drops. */
    commit(outputs: VuMeterDef[]): void {
        this.set({
            ghosts:
                this.snapshot.ghosts.size === 0
                    ? this.snapshot.ghosts
                    : new Map(),
            outputs,
        });
    }

    /** Set or clear (`value` undefined) one ghost property of a meter. */
    setGhostProp<P extends keyof VuMeterGhost>(
        key: string,
        prop: P,
        value: VuMeterGhost[P] | undefined,
    ): void {
        const prev = this.snapshot.ghosts;
        if (value === undefined && prev.get(key)?.[prop] === undefined) {
            return;
        }
        const entry: VuMeterGhost = { ...prev.get(key) };
        if (value === undefined) {
            delete entry[prop];
        } else {
            entry[prop] = value;
        }
        const ghosts = new Map(prev);
        if (Object.keys(entry).length === 0) {
            ghosts.delete(key);
        } else {
            ghosts.set(key, entry);
        }
        this.set({ ...this.snapshot, ghosts });
    }

    private set(snapshot: MixerSnapshot): void {
        this.snapshot = snapshot;
        for (const listener of this.listeners) {
            listener();
        }
    }
}

export function useMixerSnapshot(store: MixerStore): MixerSnapshot {
    return useSyncExternalStore(store.subscribe, store.getSnapshot);
}
