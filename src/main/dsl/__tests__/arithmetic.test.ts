/**
 * `.add()`, `.sub()`, `.mul()`, `.div()` and `.exp()` are the chain methods of
 * `$add`, `$sub`, `$mul`, `$div` and `$exp`; `.round()`, `.floor()` and
 * `.ceil()` build a hidden `_round`. On a ranged signal the rounding methods
 * round the range too: number bounds in JS, signal bounds through their own
 * `_round`.
 */

import { beforeEach, describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { DSLContext } from '../factories';
import {
    Collection,
    CollectionWithRange,
    type GraphBuilder,
    ModuleOutput,
    ModuleOutputWithRange,
} from '../GraphBuilder';
import { executePatchScript } from '../executor';

let builder: GraphBuilder;

beforeEach(() => {
    builder = new DSLContext(schemas).getBuilder();
});

function modules(moduleType: string) {
    return builder.toPatch().modules.filter((m) => m.moduleType === moduleType);
}

function sine(freq: string): Collection {
    return builder.getFactory('$sine')(freq) as Collection;
}

function run(source: string) {
    return executePatchScript(source, schemas, {
        sampleRate: 48_000,
        workspaceRoot: '/workspace',
    }).patch.modules;
}

describe('arithmetic methods', () => {
    test.each([
        ['add', '$add', 'offset'],
        ['sub', '$sub', 'offset'],
        ['mul', '$mul', 'factor'],
        ['div', '$div', 'divisor'],
        ['exp', '$exp', 'exponent'],
    ])('.%s() builds a %s with its operand', (method, moduleType, operand) => {
        const [node] = run(`$sine('1hz').${method}(3).out()`).filter(
            (m) => m.moduleType === moduleType,
        );
        expect(node.params).toMatchObject({ [operand]: 3 });
    });

    test('a missing operand says which', () => {
        expect(() => run(`$sine('1hz').exp().out()`)).toThrow(
            /missing required parameter `exponent`/,
        );
    });

    test('a collection keeps its width', () => {
        const muls = run(`$c('c3', 'e3', 'g3').mul(2).out()`).filter(
            (m) => m.moduleType === '$mul',
        );
        expect(muls).toHaveLength(1);
        expect(muls[0].params.input).toHaveLength(3);
    });

    test('an empty collection builds nothing', () => {
        expect(new CollectionWithRange().round().length).toBe(0);
        const types = builder.toPatch().modules.map((m) => m.moduleType);
        expect(types).not.toContain('_round');
    });
});

describe('rounding methods', () => {
    test.each(['round', 'floor', 'ceil'] as const)(
        '.%s() on an unranged signal stays unranged',
        (mode) => {
            const out = builder.$c('c3')[mode]();

            expect(out.items[0]).not.toBeInstanceOf(ModuleOutputWithRange);
            const [node] = modules('_round');
            expect(node.params).toMatchObject({ mode });
        },
    );

    test.each([
        ['round', -2, 4],
        ['floor', -2, 3],
        ['ceil', -1, 4],
    ] as const)('.%s() rounds number bounds', (mode, min, max) => {
        const ranged = (sine('1hz') as unknown as CollectionWithRange).range(
            -1.5,
            3.5,
        );
        const out = ranged[mode]();

        expect(out).toBeInstanceOf(CollectionWithRange);
        expect([out.items[0].minValue, out.items[0].maxValue]).toEqual([
            min,
            max,
        ]);
        expect(modules('_round')).toHaveLength(1);
    });

    test('halves round away from zero, as in the engine', () => {
        const out = (sine('1hz') as unknown as CollectionWithRange)
            .range(-2.5, 2.5)
            .round();

        expect([out.items[0].minValue, out.items[0].maxValue]).toEqual([-3, 3]);
    });

    test('a ranged output rounds its range', () => {
        const out = (sine('1hz') as unknown as CollectionWithRange)
            .range(0.4, 7.6)[0]
            .round();

        expect(out).toBeInstanceOf(CollectionWithRange);
        expect([out.items[0].minValue, out.items[0].maxValue]).toEqual([0, 8]);
    });

    test('signal bounds round through their own _round, not their methods', () => {
        // Each bound is itself ranged; rounding it must not round its range
        // in turn.
        const lo = (sine('2hz') as unknown as CollectionWithRange).range(
            0,
            1,
        )[0];
        const hi = (sine('3hz') as unknown as CollectionWithRange).range(
            4,
            5,
        )[0];
        const out = (sine('1hz') as unknown as CollectionWithRange)
            .range(lo, hi)
            .floor();

        const min = out.items[0].minValue as ModuleOutput;
        const max = out.items[0].maxValue as ModuleOutput;
        expect(min).toBeInstanceOf(ModuleOutput);
        expect(min).not.toBeInstanceOf(ModuleOutputWithRange);
        expect(max).not.toBeInstanceOf(ModuleOutputWithRange);
        const rounds = modules('_round');
        // The signal and each of its two bounds.
        expect(rounds).toHaveLength(3);
        expect(rounds.map((m) => m.params)).toContainEqual(
            expect.objectContaining({ mode: 'floor' }),
        );
        const ids = rounds.map((m) => m.id);
        expect(ids).toContain(min.moduleId);
        expect(ids).toContain(max.moduleId);
    });

    test('per-output ranges of a ranged collection round separately', () => {
        const ranged = new CollectionWithRange(
            ...(sine('1hz') as unknown as CollectionWithRange).range(0.2, 1.7)
                .items,
            ...(sine('2hz') as unknown as CollectionWithRange).range(2.5, 5.5)
                .items,
        );
        const out = ranged.ceil();

        expect(out.items.map((o) => [o.minValue, o.maxValue])).toEqual([
            [1, 2],
            [3, 6],
        ]);
    });

    test('outputs sharing a bound signal share its rounding', () => {
        const lo = builder.$c('c2')[0];
        const hi = builder.$c('c5')[0];
        const out = (
            builder.getFactory('$sine')([
                '1hz',
                '2hz',
                '3hz',
                '4hz',
            ]) as unknown as CollectionWithRange
        )
            .range(lo, hi)
            .round();

        expect(out.length).toBe(4);
        // The signal and one rounding per bound.
        expect(modules('_round')).toHaveLength(3);
    });

    test('_round is not a global', () => {
        expect(() => run('$round(0).out()')).toThrow();
    });
});
