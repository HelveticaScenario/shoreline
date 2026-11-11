/**
 * Contextual outputs. The value a multi-output module's factory returns is a
 * root: it stands for whichever of the module's outputs fits the input it is
 * plugged into. A named output (`seq.trig`) is never a root, so naming one is
 * always explicit.
 */

import type { ModuleSchema } from '@modular/core';
import {
    chooseOutput,
    type OutputClass,
    type OutputResolutionRecord,
    type ResolvableOutput,
} from '../../shared/dsl/outputResolutionRule';
import { captureSourceLocation } from './captureSourceLocation';

const OUTPUT_SET = Symbol('outputSet');

/** A module's outputs, keyed by the property names they have on the root. */
interface OutputSet {
    outputs: readonly ResolvableOutput[];
    values: Readonly<Record<string, unknown>>;
}

/** The outputs of `schema` under the property names `propertyName` gives them. */
export function resolvableOutputs(
    schema: ModuleSchema,
    propertyName: (output: string) => string,
): ResolvableOutput[] {
    return (schema.outputs ?? []).map((o) => ({
        default: o.default,
        name: propertyName(o.name),
        outputType: o.outputType,
    }));
}

/** A call's position in the patch source, as `captureSourceLocation` reads it. */
export type CallSite = { line: number; column: number };

/** Tag `root` as standing for every output in `set`. */
export function markRoot(root: object, set: OutputSet): void {
    Object.defineProperty(root, OUTPUT_SET, { value: set });
}

function outputSetOf(value: unknown): OutputSet | undefined {
    if (typeof value !== 'object' || value === null) {
        return undefined;
    }
    return (value as { [OUTPUT_SET]?: OutputSet })[OUTPUT_SET];
}

/**
 * Resolves roots to outputs and records each choice against the call site
 * and slot the editor's hint collector knows it by.
 */
export class OutputResolver {
    readonly records: OutputResolutionRecord[] = [];
    private inChainCall = false;

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
        if (!this.inChainCall) {
            this.record(site, slot, name);
        }
        return set.values[name];
    }

    /**
     * Run a chain method's body as one user-facing call. A chain method reads
     * its receiver and its signal arguments as `cv`, so the roots among them
     * are recorded here against that call, and the modules the body builds
     * record nothing.
     */
    withChainCall<T>(
        receiver: unknown,
        args: readonly unknown[],
        body: () => T,
    ): T {
        if (this.inChainCall) {
            return body();
        }
        const roots: [slot: string, set: OutputSet][] = [];
        const collect = (slot: string, value: unknown): void => {
            const set = outputSetOf(value);
            if (set) {
                roots.push([slot, set]);
            }
        };
        collect('receiver', receiver);
        args.forEach((arg, i) => {
            collect(String(i), arg);
            if (Array.isArray(arg)) {
                arg.forEach((element, j) => collect(`${i}.${j}`, element));
            }
        });
        if (roots.length > 0) {
            const site = captureSourceLocation();
            for (const [slot, set] of roots) {
                this.record(site, slot, chooseOutput(set.outputs, 'cv'));
            }
        }
        this.inChainCall = true;
        try {
            return body();
        } finally {
            this.inChainCall = false;
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
