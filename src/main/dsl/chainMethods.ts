/**
 * The chain methods of outputs and collections, each run as one user-facing
 * call so the module roots it reads are recorded against that call.
 * Imported for its effect on the prototypes.
 */

import {
    BaseCollection,
    Collection,
    CollectionWithRange,
    ModuleOutput,
    ModuleOutputWithRange,
    type GraphBuilder,
} from './GraphBuilder';

/**
 * Methods a user calls on an output or collection that read it, and their
 * signal arguments, as `cv`. `pipe` and `pipeMix` are absent: they run user
 * code, whose own calls are separate call sites.
 */
export const CHAIN_METHODS = [
    'amplitude',
    'amp',
    'shift',
    'addHz',
    'mulHz',
    'gain',
    'exp',
    'add',
    'sub',
    'mul',
    'div',
    'round',
    'floor',
    'ceil',
    'erratic',
    'scope',
    'out',
    'outMono',
    'send',
    'mix',
    'range',
] as const;

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

for (const proto of [
    ModuleOutput.prototype,
    ModuleOutputWithRange.prototype,
    BaseCollection.prototype,
    Collection.prototype,
    CollectionWithRange.prototype,
] as object[]) {
    for (const name of CHAIN_METHODS) {
        if (!Object.hasOwn(proto, name)) {
            continue;
        }
        const method = (proto as Record<string, ChainMethod>)[name];
        (proto as Record<string, ChainMethod>)[name] = function (...args) {
            const builder = builderOf(this);
            return builder
                ? builder.outputResolver.withChainCall(this, args, () =>
                      method.apply(this, args),
                  )
                : method.apply(this, args);
        };
    }
}
