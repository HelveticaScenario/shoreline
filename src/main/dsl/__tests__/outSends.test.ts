/**
 * An out's `sends` feeds the out's signal to each bus named, at full level
 * or at a pair's gain, before the out's own gain and pan.
 */

import { describe, expect, test } from 'vitest';
import type { PatchGraph } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';

const OPTIONS = { sampleRate: 48_000, workspaceRoot: '/workspace' } as const;

type Cable = { module: string; type: 'cable' };

function patchOf(source: string): PatchGraph {
    return executePatchScript(source, schemas, OPTIONS).patch;
}

/**
 * What the `index`th bus mixes, as `source` or `source@level` per send. A
 * bus builds its mix before the outs build theirs, so the buses' mixes are
 * the first `$mix` modules, in the order the buses were made.
 */
function busSends(patch: PatchGraph, index: number): string[] {
    const byId = new Map(patch.modules.map((m) => [m.id, m]));
    const mix = byId.get(`$mix-${index + 1}`)!;
    const inputs = (mix.params as { inputs: (Cable | Cable[])[] }).inputs;
    return inputs.map((input) => {
        const cable = Array.isArray(input) ? input[0] : input;
        const from = byId.get(cable.module)!;
        if (from.moduleType !== '$gain') {
            return from.id;
        }
        const { input: gained, level } = from.params as {
            input: Cable[];
            level: unknown;
        };
        return `${gained[0].module}@${JSON.stringify(level)}`;
    });
}

const BUSES = `const a = $bus((m) => m.outMono(2)); const b = $bus((m) => m.outMono(3));`;

describe('out sends', () => {
    test.each([
        [`$saw('c3').out({ sends: a })`, ['$saw-1'], []],
        [`$saw('c3').out({ sends: [a, 3] })`, ['$saw-1@3'], []],
        [`$saw('c3').out({ sends: [a, b] })`, ['$saw-1'], ['$saw-1']],
        [
            `$saw('c3').out({ sends: [[a, 2], [b, 3]] })`,
            ['$saw-1@2'],
            ['$saw-1@3'],
        ],
        [`$saw('c3').out({ sends: [a, [b, 4]] })`, ['$saw-1'], ['$saw-1@4']],
        [`$saw('c3').out({ sends: [[a, 2]] })`, ['$saw-1@2'], []],
        [`$saw('c3').out({ sends: [a, [1, 2]] })`, ['$saw-1@[1,2]'], []],
        [
            `$saw('c3').outMono(0, { sends: [a, [b, 4]] })`,
            ['$saw-1'],
            ['$saw-1@4'],
        ],
        [`$saw('c3').outMono({ sends: [b, 1] })`, [], ['$saw-1@1']],
    ])('%s', (patch, toA, toB) => {
        const built = patchOf(`${BUSES} ${patch}`);
        expect(busSends(built, 0)).toEqual(toA);
        expect(busSends(built, 1)).toEqual(toB);
    });

    test('every out sending to a bus is mixed into it', () => {
        const built = patchOf(
            `${BUSES} $saw('c3').out({ sends: a }); $sine('c4').outMono(1, { sends: [a, 2] })`,
        );
        expect(busSends(built, 0)).toEqual(['$saw-1', '$sine-1@2']);
    });

    test.each([
        [`$saw('c3').out({ sends: 3 })`],
        [`$saw('c3').out({ sends: [a, 2, 3] })`],
        [`$saw('c3').out({ sends: [[a, 2], 3] })`],
    ])('%s is rejected', (patch) => {
        expect(() => patchOf(`${BUSES} ${patch}`)).toThrow(/sends/);
    });

    test('a bus callback cannot send', () => {
        expect(() =>
            patchOf(
                `const a = $bus((m) => m.out()); const b = $bus((m) => m.out({ sends: a })); $saw('c3').out({ sends: b })`,
            ),
        ).toThrow('`sends` is not allowed in $bus callbacks');
    });
});
