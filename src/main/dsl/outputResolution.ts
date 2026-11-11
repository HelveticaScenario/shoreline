/**
 * Contextual outputs. The value a multi-output module's factory returns is a
 * root: it stands for whichever of the module's outputs fits the input it is
 * plugged into. A named output (`seq.o.trig`) is never a root, so naming one is
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
import { sanitizeOutputName } from './outputNames';
import { isSignalGroupLike } from './signalGroups';

const OUTPUT_SET = Symbol('outputSet');

/** A module's outputs, keyed by their names on the root's `o`. */
interface OutputSet {
    outputs: readonly ResolvableOutput[];
    values: Readonly<Record<string, unknown>>;
}

/** The outputs of `schema` under their names on a root's `o`. */
export function resolvableOutputs(schema: ModuleSchema): ResolvableOutput[] {
    return (schema.outputs ?? []).map((o) => ({
        default: o.default,
        name: sanitizeOutputName(o.name),
        outputType: o.outputType,
    }));
}

/** A call's position in the patch source, as `captureSourceLocation` reads it. */
export type CallSite = { line: number; column: number };

/** Tag `root` as standing for every output in `set`. */
export function markRoot(root: object, set: OutputSet): void {
    Object.defineProperty(root, OUTPUT_SET, { value: set });
}

/** Whether `value` is a multi-output module's root. */
export function isRoot(value: unknown): boolean {
    return outputSetOf(value) !== undefined;
}

function outputSetOf(value: unknown): OutputSet | undefined {
    if (typeof value !== 'object' || value === null) {
        return undefined;
    }
    return (value as { [OUTPUT_SET]?: OutputSet })[OUTPUT_SET];
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
        const name = chooseOutput(set.outputs, inputClass);
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
                this.record(site, slot, chooseOutput(set.outputs, inputClass));
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
