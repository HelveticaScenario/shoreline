/**
 * A dotted module's leaf lives under its namespace, so it never collides with
 * a flat module of the same name: `sig.ns.fold()` and `sig.fold()` reach
 * different modules.
 */

import { describe, expect, test } from 'vitest';
import type { ModuleSchema } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import { createChainInstaller } from '../chainModules';
import { processModuleSchema } from '../paramsSchema';

const fold = processModuleSchema(
    schemas.find((s) => s.name === '$fold') as unknown as ModuleSchema,
);

/** A receiver whose chain calls report the path they resolved to. */
function receiverWith(names: readonly string[]): Record<string, unknown> {
    const prototype = {};
    createChainInstaller({
        prototypes: [prototype],
        subclasses: [],
        hostOf: () => ({
            chainFunction: (path) => () => ({ path }),
        }),
        empty: () => undefined,
    }).addModules(names.map((name) => ({ ...fold, name })));
    return Object.create(prototype) as Record<string, unknown>;
}

const callPath = (method: unknown): unknown =>
    (method as () => { path: string })().path;

describe('chain namespaces', () => {
    for (const names of [
        ['$fold', '$ns.fold'],
        ['$ns.fold', '$fold'],
    ]) {
        test(`a leaf and a flat module of one name stay apart (${names.join(', ')})`, () => {
            const sig = receiverWith(names);
            const ns = sig.ns as Record<string, unknown>;
            expect(callPath(sig.fold)).toBe('fold');
            expect(callPath(ns.fold)).toBe('ns.fold');
        });
    }

    test('a namespaced leaf adds no flat method', () => {
        expect(receiverWith(['$ns.fold']).fold).toBeUndefined();
    });
});
