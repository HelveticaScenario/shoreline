/**
 * `.erratic()` builds a hidden `_erratic` module. On a ranged signal it leaves
 * the range and mode to the engine, which varies within the signal's range
 * and blends by default, and the result keeps the range unless it shifts; on
 * an unranged signal it shifts across a nominal ±5 V.
 */

import { beforeEach, describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { DSLContext } from '../factories';
import {
    Collection,
    CollectionWithRange,
    type GraphBuilder,
    ModuleOutputWithRange,
} from '../GraphBuilder';
import { executePatchScript } from '../executor';

let builder: GraphBuilder;

beforeEach(() => {
    builder = new DSLContext(schemas).getBuilder();
});

function erraticParams(): Record<string, unknown> {
    const node = builder
        .toPatch()
        .modules.find((m) => m.moduleType === '_erratic');
    expect(node).toBeDefined();
    return node!.params as Record<string, unknown>;
}

describe('.erratic()', () => {
    test('a ranged output leaves its range and mode to the engine', () => {
        const out = (builder.getFactory('$sine')('1hz') as CollectionWithRange)
            .range(0, 10)[0]
            .erratic(2, { speed: 3 });

        expect(out).toBeInstanceOf(CollectionWithRange);
        const params = erraticParams();
        expect(params.speed).toBe(3);
        expect(params).not.toHaveProperty('min');
        expect(params).not.toHaveProperty('max');
        expect(params).not.toHaveProperty('mode');
    });

    test("mode: 'shift' on a ranged collection gives up the range", () => {
        const out = (builder.getFactory('$sine')('1hz') as CollectionWithRange)
            .range(-3, -1)
            .erratic(5, { mode: 'shift' });

        expect(out).toBeInstanceOf(Collection);
        expect(out.items[0]).not.toBeInstanceOf(ModuleOutputWithRange);
        expect(erraticParams()).toMatchObject({ mode: 'shift' });
    });

    test("mode: 'clamp' on a ranged collection keeps the range", () => {
        const out = (builder.getFactory('$sine')('1hz') as CollectionWithRange)
            .range(-3, -1)
            .erratic(5, { mode: 'clamp' });

        expect(out).toBeInstanceOf(CollectionWithRange);
        expect(erraticParams()).toMatchObject({ mode: 'clamp' });
    });

    test('unranged signal stays unranged', () => {
        const out = builder.$c('c3').erratic(1, { bias: 2 });

        expect(out).toBeInstanceOf(Collection);
        expect(out.items[0]).not.toBeInstanceOf(ModuleOutputWithRange);
        expect(erraticParams().bias).toBe(2);
    });

    test('mode and sparse pass through on a ranged signal', () => {
        (builder.getFactory('$sine')('1hz') as CollectionWithRange)
            .range(0, 5)
            .erratic(5, { mode: 'blend', sparse: true });
        expect(erraticParams()).toMatchObject({
            mode: 'blend',
            sparse: true,
        });
    });

    test('unranged signal passes sparse and shifts across a nominal ±5 V', () => {
        builder.$c('c3').erratic(1, { sparse: true, mode: 'blend' } as object);
        expect(erraticParams()).toMatchObject({
            sparse: true,
            mode: 'shift',
            min: -5,
            max: 5,
        });
    });

    test('_erratic is not a global', () => {
        const run = (src: string) =>
            executePatchScript(src, schemas, {
                sampleRate: 48_000,
                workspaceRoot: '/workspace',
            });
        expect(() => run('$erratic(0, 1).out()')).toThrow();
    });
});
