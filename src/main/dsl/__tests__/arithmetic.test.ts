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
        const types = run(`$c().add(1).mul(2).out()`).map((m) => m.moduleType);
        expect(types).not.toContain('$add');
        expect(types).not.toContain('$mul');
        expect(new CollectionWithRange().round().length).toBe(0);
        expect(
            builder.toPatch().modules.map((m) => m.moduleType),
        ).not.toContain('_round');
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

    test.each(['round', 'floor', 'ceil'] as const)(
        '.%s() keeps a ranged signal ranged through one _round',
        (mode) => {
            const out = sine('1hz').range(-1.5, 3.5, -5, 5)[mode]();

            expect(out).toBeInstanceOf(CollectionWithRange);
            const rounds = modules('_round');
            expect(rounds).toHaveLength(1);
            expect(rounds[0].params).toMatchObject({ mode });
        },
    );

    test('a ranged channel rounds ranged', () => {
        const out = (sine('1hz') as unknown as CollectionWithRange)
            .range(0.4, 7.6)[0]
            .round();

        expect(out).toBeInstanceOf(CollectionWithRange);
    });

    test('rounding a ranged collection rounds every channel at once', () => {
        const out = (
            builder.getFactory('$sine')([
                '1hz',
                '2hz',
            ]) as unknown as CollectionWithRange
        ).ceil();

        expect(out.length).toBe(2);
        expect(modules('_round')).toHaveLength(1);
    });

    test('_round is not a global', () => {
        expect(() => run('$round(0).out()')).toThrow();
    });
});
