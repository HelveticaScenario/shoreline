/**
 * The chain methods of collections, each run as one user-facing
 * call so the module roots it reads are recorded against that call.
 * Imported for its effect on the prototypes.
 */

import {
    argumentSlot,
    optionSlot,
    RECEIVER_SLOT,
    type OutputClass,
} from '../../shared/dsl/outputResolutionRule';
import type { ChainMethods } from '../../shared/dsl/signalSites';
import {
    BaseCollection,
    Collection,
    CollectionWithRange,
    type GraphBuilder,
} from './GraphBuilder';
import type { CallSlot } from './outputResolution';
import { isPlainObject } from './plainObject';
import { isSignalGroupLike } from './signalGroups';

/**
 * `pipe` and `pipeMix` are absent: they run user code, whose own calls are
 * separate call sites.
 */
export const CHAIN_METHODS: ChainMethods = {
    out: {},
    outMono: {},
    range: {},
    scope: {},
    send: {},
    wet: {},
};

type ChainMethod = (this: unknown, ...args: unknown[]) => unknown;

function builderOf(receiver: unknown): GraphBuilder | undefined {
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
        isPlainObject(arg) && !isSignalGroupLike(arg)
            ? Object.entries(arg).map(([key, value]) => [
                  optionSlot(i, key),
                  value,
                  optionClasses[key] ?? 'cv',
              ])
            : [[argumentSlot(i), arg, 'cv']],
    );
}

for (const proto of [
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
                          [RECEIVER_SLOT, this, 'cv'],
                          ...slotsOf(args, optionClasses),
                      ],
                      () => method.apply(this, args),
                  )
                : method.apply(this, args);
        };
    }
}
