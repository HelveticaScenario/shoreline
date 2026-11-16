/**
 * Contextual outputs. The value a multi-output module's factory returns is a
 * root: it stands for whichever of the module's outputs fits the input it is
 * plugged into. A named output (`seq._trig`) is never a root, so naming one is
 * always explicit.
 */

import type { ModuleSchema } from '@modular/core';
import {
    chooseOutput,
    elementSlot,
    type OutputClass,
    type OutputResolutionRecord,
    type ResolvableOutput,
} from '../../shared/dsl/outputResolutionRule';
import { captureSourceLocation } from './captureSourceLocation';
import { outputName } from './outputNames';
import { patternSignal } from './patterns';
import { isSignalGroupLike } from './signalGroups';

const OUTPUT_SET = Symbol('outputSet');

/** A module's outputs by name; the root has each as `_<name>`. */
export interface OutputSet {
    outputs: readonly ResolvableOutput[];
    values: Readonly<Record<string, unknown>>;
    /** The resolver that tracks which output the root is first taken as. */
    resolver: OutputResolver;
    /** The roots this one was sliced or joined from; taking it takes them. */
    parents?: readonly OutputSet[];
}

/** The outputs of `schema` under their names on a root. */
export function resolvableOutputs(schema: ModuleSchema): ResolvableOutput[] {
    return (schema.outputs ?? []).map((o) => ({
        default: o.default,
        name: outputName(o.name),
        outputType: o.outputType,
    }));
}

/** A call's position in the patch source, as `captureSourceLocation` reads it. */
export type CallSite = { line: number; column: number };

/**
 * Tag `root` as standing for every output in `set`, and give it each output
 * by name behind an underscore (`seq._trig`). Reading one by name takes the
 * root as that output.
 */
export function markRoot(root: object, set: OutputSet): void {
    Object.defineProperty(root, OUTPUT_SET, { value: set });
    for (const [name, value] of Object.entries(set.values)) {
        Object.defineProperty(root, `_${name}`, {
            get() {
                set.resolver.take(set, name);
                return value;
            },
        });
    }
}

/**
 * The outputs a multi-output module's root stands for, if `value` is one. A
 * pattern is the root of its `$cycle`.
 */
export function outputSetOf(value: unknown): OutputSet | undefined {
    const root = patternSignal(value);
    if (typeof root !== 'object' || root === null) {
        return undefined;
    }
    return (root as { [OUTPUT_SET]?: OutputSet })[OUTPUT_SET];
}

/**
 * A signal value of a user-facing call: the slot the hint collector knows it
 * by, the value, and the class of the input it feeds.
 */
export type CallSlot = [slot: string, value: unknown, inputClass: OutputClass];

/**
 * Resolves roots to outputs and records each choice against the call site
 * and slot the editor's hint collector knows it by.
 */
export class OutputResolver {
    readonly records: OutputResolutionRecord[] = [];
    private inCall = false;
    private firstChoices = new Map<OutputSet, string>();

    /** The output `set`'s root was first taken as, if it was. */
    firstChoice(set: OutputSet): string | undefined {
        return this.firstChoices.get(set);
    }

    /** Note that `set`'s root, and the roots it came from, were taken as `name`. */
    take(set: OutputSet, name: string): void {
        if (!this.firstChoices.has(set)) {
            this.firstChoices.set(set, name);
        }
        for (const parent of set.parents ?? []) {
            this.take(parent, name);
        }
    }

    /**
     * The output `value` contributes to an input of class `inputClass`;
     * anything but a root passes through.
     */
    resolve(
        value: unknown,
        inputClass: OutputClass,
        site: CallSite | undefined,
        slot: string,
    ): unknown {
        const set = outputSetOf(value);
        if (!set) {
            return value;
        }
        const name = this.choose(set, inputClass);
        if (!this.inCall) {
            this.record(site, slot, name);
        }
        return set.values[name];
    }

    /**
     * Run `body` as one user-facing call that builds its modules out of
     * sight: a chain method or a helper such as `$delay`. The roots among
     * `slots`, and inside the arrays and `$gN` groups among them, are
     * recorded here against that call, and the modules the body builds
     * record nothing.
     */
    withCall<T>(slots: readonly CallSlot[], body: () => T): T {
        if (this.inCall) {
            return body();
        }
        const roots: [string, OutputSet, OutputClass][] = [];
        for (const [slot, slotValue, inputClass] of slots) {
            const value = isSignalGroupLike(slotValue)
                ? slotValue.signals
                : slotValue;
            const values: [string, unknown][] = Array.isArray(value)
                ? value.map((element, i) => [elementSlot(slot, i), element])
                : [[slot, value]];
            for (const [s, v] of values) {
                const set = outputSetOf(v);
                if (set) {
                    roots.push([s, set, inputClass]);
                }
            }
        }
        if (roots.length > 0) {
            const site = captureSourceLocation();
            for (const [slot, set, inputClass] of roots) {
                this.record(site, slot, this.choose(set, inputClass));
            }
        }
        this.inCall = true;
        try {
            return body();
        } finally {
            this.inCall = false;
        }
    }

    /**
     * Run user code called back from inside a call: the calls it makes are
     * its own call sites and record as usual.
     */
    withUserCode<T>(body: () => T): T {
        const { inCall } = this;
        this.inCall = false;
        try {
            return body();
        } finally {
            this.inCall = inCall;
        }
    }

    private choose(set: OutputSet, inputClass: OutputClass): string {
        const name = chooseOutput(set.outputs, inputClass);
        this.take(set, name);
        return name;
    }

    private record(
        site: CallSite | undefined,
        slot: string,
        output: string,
    ): void {
        if (site) {
            this.records.push({
                column: site.column,
                line: site.line,
                output,
                slot,
            });
        }
    }
}
