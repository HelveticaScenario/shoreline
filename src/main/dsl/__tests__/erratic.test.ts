/**
 * `.erratic()` is the chain method of `$erratic`. Unset, its `mode` blends
 * within a signal's range and shifts a signal without one; the result is
 * ranged when the signal is and the mode keeps its range, as the engine
 * bounds it.
 */

import { beforeEach, describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { DSLContext } from '../factories';
import {
    type Collection,
    CollectionWithRange,
    type GraphBuilder,
} from '../GraphBuilder';
import { executePatchScript } from '../executor';

let builder: GraphBuilder;

beforeEach(() => {
    builder = new DSLContext(schemas).getBuilder();
});

/** Call chain method `method`, which the builder installs at runtime. */
function chain(receiver: unknown, method: string, ...args: unknown[]) {
    const methods = receiver as Record<string, (...a: unknown[]) => unknown>;
    return methods[method](...args);
}

function erraticParams(): Record<string, unknown> {
    const node = builder
        .toPatch()
        .modules.find((m) => m.moduleType === '$erratic');
    expect(node).toBeDefined();
    return node!.params as Record<string, unknown>;
}

const lfo = () =>
    (builder.getFactory('$sine')('1hz') as Collection).range(0, 10, -5, 5);

describe('.erratic()', () => {
    test('a ranged signal stays ranged, its range and mode left to the engine', () => {
        const out = chain(lfo(), 'erratic', 2, { speed: 3 });

        expect(out).toBeInstanceOf(CollectionWithRange);
        const params = erraticParams();
        expect(params.speed).toBe(3);
        expect(params).not.toHaveProperty('min');
        expect(params).not.toHaveProperty('max');
        expect(params).not.toHaveProperty('mode');
    });

    test("mode: 'shift' gives up the range", () => {
        const out = chain(lfo(), 'erratic', 5, { mode: 'shift' });

        expect(out).not.toBeInstanceOf(CollectionWithRange);
        expect(erraticParams()).toMatchObject({ mode: 'shift' });
    });

    test.each(['clamp', 'blend'])(
        "mode: '%s' keeps a ranged signal's range",
        (mode) => {
            expect(chain(lfo(), 'erratic', 5, { mode })).toBeInstanceOf(
                CollectionWithRange,
            );
        },
    );

    test.each(['clamp', 'blend'])(
        "mode: '%s' gives a signal without a range none",
        (mode) => {
            expect(
                chain(builder.$c('c3'), 'erratic', 5, { mode }),
            ).not.toBeInstanceOf(CollectionWithRange);
            expect(erraticParams()).toMatchObject({ mode });
        },
    );

    test('a constant signal has no range, so it shifts', () => {
        const out = chain(builder.$c('c3'), 'erratic', 1, { bias: 2 });

        expect(out).not.toBeInstanceOf(CollectionWithRange);
        const params = erraticParams();
        expect(params.bias).toBe(2);
        expect(params).not.toHaveProperty('mode');
    });

    test('$erratic is a global', () => {
        const run = (src: string) =>
            executePatchScript(src, schemas, {
                sampleRate: 48_000,
                workspaceRoot: '/workspace',
            });
        const types = run('$erratic($sine("1hz"), 1).out()').patch.modules.map(
            (m) => m.moduleType,
        );
        expect(types).toContain('$erratic');
    });
});
