/**
 * Contextual outputs. The value a multi-output module's factory returns is a
 * root: it stands for whichever of the module's outputs fits the input it is
 * plugged into. A named output (`seq.trig`) is never a root, so naming one is
 * always explicit.
 */

import {
    chooseOutput,
    type OutputClass,
    type OutputResolutionRecord,
    type ResolvableOutput,
} from '../../shared/dsl/outputResolutionRule';
import { captureSourceLocation } from './captureSourceLocation';

const OUTPUT_SET = Symbol('outputSet');

/** A module's outputs, keyed by the property names they have on the root. */
interface OutputSet<V> {
    outputs: readonly ResolvableOutput[];
    values: Readonly<Record<string, V>>;
}

type SourceLocation = { line: number; column: number };

/** Tag `root` as standing for every output in `set`. */
export function markRoot<V>(root: object, set: OutputSet<V>): void {
    Object.defineProperty(root, OUTPUT_SET, { value: set });
}

function outputSetOf(value: unknown): OutputSet<unknown> | undefined {
    if (typeof value !== 'object' || value === null) {
        return undefined;
    }
    return (value as { [OUTPUT_SET]?: OutputSet<unknown> })[OUTPUT_SET];
}

/** The user-facing call a chain method (`.amp`, `.out`, …) is running for. */
interface ChainCall {
    receiver: unknown;
    args: readonly unknown[];
    site: SourceLocation | undefined;
}

/**
 * Resolves roots to outputs and records each choice against the call site
 * and slot the editor's hint collector knows it by.
 */
export class OutputResolver {
    readonly records: OutputResolutionRecord[] = [];
    private chain: ChainCall | null = null;

    /**
     * The output `value` contributes to an input of class `inputClass`;
     * anything but a root passes through.
     */
    resolve(
        value: unknown,
        inputClass: OutputClass,
        site: SourceLocation | undefined,
        slot: string,
    ): unknown {
        const set = outputSetOf(value);
        if (!set) {
            return value;
        }
        const name = chooseOutput(set.outputs, inputClass);
        if (this.chain) {
            const chainSlot = this.chainArgSlot(value);
            if (chainSlot !== undefined) {
                this.record(this.chain.site ?? site, chainSlot, name);
            }
        } else {
            this.record(site, slot, name);
        }
        return set.values[name];
    }

    /**
     * Run a chain method's body as the user-facing call on `receiver` with
     * `args`, so roots resolved inside it are recorded against that call.
     * Every chain method reads its receiver as `cv`.
     */
    withChainCall<T>(
        receiver: unknown,
        args: readonly unknown[],
        body: () => T,
    ): T {
        if (this.chain) {
            return body();
        }
        const set = outputSetOf(receiver);
        const site = set ? captureSourceLocation() : undefined;
        if (set) {
            this.record(site, 'receiver', chooseOutput(set.outputs, 'cv'));
        }
        this.chain = { args, receiver, site };
        try {
            return body();
        } finally {
            this.chain = null;
        }
    }

    private chainArgSlot(value: unknown): string | undefined {
        const { args } = this.chain!;
        for (let i = 0; i < args.length; i++) {
            const arg = args[i];
            if (arg === value) {
                return String(i);
            }
            if (Array.isArray(arg)) {
                const j = arg.indexOf(value);
                if (j !== -1) {
                    return `${i}.${j}`;
                }
            }
        }
        return undefined;
    }

    private record(
        site: SourceLocation | undefined,
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
