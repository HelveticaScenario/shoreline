/**
 * Chain modules: every module whose first argument is a signal is also a
 * method of collections, with the receiver as that argument, so
 * `sig.lpf('800hz')` is `$lpf(sig, '800hz')`. A dotted module nests the same
 * way: `sig.mi.chorus()` is `$mi.chorus(sig)`.
 *
 * The methods live on the collection prototype, which every builder shares, and
 * each call finds its function through the receiver's own builder.
 */

import type { ProcessedModuleSchema } from './paramsSchema';
import { qualifiesForChain } from './paramsSchema';

/** A module factory or DSL helper that takes the chained signal first. */
export type ChainFunction = (self: unknown, ...args: unknown[]) => unknown;

/** What a receiver's builder provides to its chain calls. */
export interface ChainHost {
    /** The function behind chain path `path` (`lpf`, `mi.chorus`). */
    chainFunction(path: string): ChainFunction;
}

export interface ChainTarget {
    /** The prototypes that take the chain methods. */
    prototypes: readonly object[];
    /** Prototypes below those, whose members would shadow a chain method. */
    subclasses: readonly object[];
    /** The host of `receiver`, or undefined for an empty collection. */
    hostOf(receiver: unknown): ChainHost | undefined;
    /** What a chain call on a receiver without a host returns. */
    empty(): unknown;
}

/** The chain paths below one namespace segment. */
interface ChainNode {
    leaves: Set<string>;
    children: Map<string, ChainNode>;
}

const DRY = new WeakMap<object, unknown>();

/** The receiver of the chain call that made `value`, which `.wet()` fades against. */
export function drySourceOf(value: object): unknown {
    return DRY.get(value);
}

/**
 * Record `dry` as the source of `result` and of each named output on its
 * `o`, so `.wet()` works on a root and on an output named explicitly.
 */
function markWet(result: unknown, dry: unknown): void {
    if (typeof result !== 'object' || result === null) {
        return;
    }
    DRY.set(result, dry);
    const named = (result as { o?: unknown }).o;
    if (typeof named === 'object' && named !== null) {
        for (const output of Object.values(named)) {
            if (typeof output === 'object' && output !== null) {
                DRY.set(output, dry);
            }
        }
    }
}

export interface ChainInstaller {
    /** Install a method for every chainable module among `schemas`. */
    addModules(schemas: readonly ProcessedModuleSchema[]): void;
    /** Install chain method `name` for a DSL helper the host provides. */
    addMethod(name: string): void;
}

export function createChainInstaller(target: ChainTarget): ChainInstaller {
    const root: ChainNode = { children: new Map(), leaves: new Set() };
    const installed = new Set<string>();

    const call = (receiver: unknown, path: string, args: unknown[]) => {
        const host = target.hostOf(receiver);
        if (!host) {
            return target.empty();
        }
        const result = host.chainFunction(path)(receiver, ...args);
        markWet(result, receiver);
        return result;
    };

    const namespace = (
        receiver: unknown,
        node: ChainNode,
        path: string,
    ): unknown =>
        // Unknown names and symbols read as undefined, so `then` and
        // iterator probes stay inert.
        new Proxy(
            {},
            {
                get: (_target, prop) => {
                    if (typeof prop !== 'string') {
                        return undefined;
                    }
                    const next = `${path}.${prop}`;
                    if (node.leaves.has(prop)) {
                        return (...args: unknown[]) =>
                            call(receiver, next, args);
                    }
                    const child = node.children.get(prop);
                    return child ? namespace(receiver, child, next) : undefined;
                },
            },
        );

    const define = (name: string, descriptor: PropertyDescriptor): void => {
        if (installed.has(name)) {
            return;
        }
        for (const proto of [...target.prototypes, ...target.subclasses]) {
            if (name in proto) {
                throw new Error(
                    `Chain method "${name}" collides with a built-in member of a collection`,
                );
            }
        }
        for (const proto of target.prototypes) {
            Object.defineProperty(proto, name, {
                configurable: true,
                ...descriptor,
            });
        }
        installed.add(name);
    };

    const addMethod = (name: string): void =>
        define(name, {
            value(this: unknown, ...args: unknown[]) {
                return call(this, name, args);
            },
            writable: true,
        });

    return {
        addMethod,
        addModules(schemas) {
            for (const schema of schemas) {
                if (!qualifiesForChain(schema)) {
                    continue;
                }
                const segments = schema.name.slice(1).split('.');
                const leaf = segments.pop()!;
                if (segments.length === 0) {
                    addMethod(leaf);
                    continue;
                }
                let node = root;
                for (const segment of segments) {
                    let child = node.children.get(segment);
                    if (!child) {
                        child = { children: new Map(), leaves: new Set() };
                        node.children.set(segment, child);
                    }
                    node = child;
                }
                node.leaves.add(leaf);
                const [head] = segments;
                const headNode = root.children.get(head)!;
                define(head, {
                    get(this: unknown) {
                        return namespace(this, headNode, head);
                    },
                });
            }
        },
    };
}
