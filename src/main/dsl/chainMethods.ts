/**
 * The chain methods of outputs and collections, each run as one user-facing
 * call so the module roots it reads are recorded against that call.
 * Imported for its effect on the prototypes.
 */

import type { OutputClass } from '../../shared/dsl/outputResolutionRule';
import {
    BaseCollection,
    Collection,
    CollectionWithRange,
    isPlainObject,
    ModuleOutput,
    ModuleOutputWithRange,
    type GraphBuilder,
} from './GraphBuilder';
import type { CallSlot } from './outputResolution';

/**
 * Methods a user calls on an output or collection that read it, and the
 * signals among their arguments, as `cv`. Each lists the fields of its
 * options object that read another class. `pipe` and `pipeMix` are absent:
 * they run user code, whose own calls are separate call sites.
 */
export const CHAIN_METHODS: Readonly<
    Record<string, Readonly<Record<string, OutputClass>>>
> = {
    add: {},
    addHz: {},
    amp: {},
    amplitude: {},
    ceil: {},
    div: {},
    erratic: { trigger: 'trig' },
    exp: {},
    floor: {},
    gain: {},
    mix: {},
    mul: {},
    mulHz: {},
    out: {},
    outMono: {},
    range: {},
    round: {},
    scope: {},
    send: {},
    shift: {},
    sub: {},
};

type ChainMethod = (this: unknown, ...args: unknown[]) => unknown;

function builderOf(receiver: unknown): GraphBuilder | undefined {
    if (receiver instanceof ModuleOutput) {
        return receiver.builder;
    }
    if (receiver instanceof BaseCollection) {
        return receiver.items[0]?.builder;
    }
    return undefined;
}

/** The slots of a chain call: its arguments and the fields of its options. */
function slotsOf(
    args: readonly unknown[],
    optionClasses: Readonly<Record<string, OutputClass>>,
): CallSlot[] {
    return args.flatMap((arg, i): CallSlot[] =>
        isPlainObject(arg)
            ? Object.entries(arg).map(([key, value]) => [
                  `${i}.${key}`,
                  value,
                  optionClasses[key] ?? 'cv',
              ])
            : [[String(i), arg, 'cv']],
    );
}

for (const proto of [
    ModuleOutput.prototype,
    ModuleOutputWithRange.prototype,
    BaseCollection.prototype,
    Collection.prototype,
    CollectionWithRange.prototype,
] as object[]) {
    for (const [name, optionClasses] of Object.entries(CHAIN_METHODS)) {
        if (!Object.hasOwn(proto, name)) {
            continue;
        }
        const method = (proto as Record<string, ChainMethod>)[name];
        (proto as Record<string, ChainMethod>)[name] = function (...args) {
            const builder = builderOf(this);
            return builder
                ? builder.outputResolver.withCall(
                      [
                          ['receiver', this, 'cv'],
                          ...slotsOf(args, optionClasses),
                      ],
                      () => method.apply(this, args),
                  )
                : method.apply(this, args);
        };
    }
}
