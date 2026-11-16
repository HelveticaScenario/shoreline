/**
 * A track's `sends` feeds the track's signal to each bus named, at full
 * level or at a pair's gain, before the track's own gain and pan.
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
 * What bus `a` (index 0) or `b` (index 1) mixes, as `source` or
 * `source@level` per send. Each bus is found from the filter chained on it.
 */
function busSends(patch: PatchGraph, index: number): string[] {
    const byId = new Map(patch.modules.map((m) => [m.id, m]));
    const filter = patch.modules.find(
        (m) => m.moduleType === (index === 0 ? '$lpf' : '$hpf'),
    )!;
    const source = (cables: unknown) =>
        byId.get((cables as Cable[])[0].module)!;
    const lanes = source(filter.params.input);
    if (lanes.moduleType !== '$setLanes') {
        return [];
    }
    const mix = source(lanes.params.input);
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

const BUSES = `a$2m: $bus().lpf('c8')\nb$3m: $bus().hpf('c0')\n`;

describe('track sends', () => {
    test.each([
        [`lead: $saw('c3').track({ sends: a })`, ['$saw-1'], []],
        [`lead: $saw('c3').track({ sends: [a, 3] })`, ['$saw-1@3'], []],
        [`lead: $saw('c3').track({ sends: [a, b] })`, ['$saw-1'], ['$saw-1']],
        [
            `lead: $saw('c3').track({ sends: [[a, 2], [b, 3]] })`,
            ['$saw-1@2'],
            ['$saw-1@3'],
        ],
        [
            `lead: $saw('c3').track({ sends: [a, [b, 4]] })`,
            ['$saw-1'],
            ['$saw-1@4'],
        ],
        [`lead: $saw('c3').track({ sends: [[a, 2]] })`, ['$saw-1@2'], []],
        [
            `lead: $saw('c3').track({ sends: [a, [1, 2]] })`,
            ['$saw-1@[1,2]'],
            [],
        ],
        [
            `lead$0m: $saw('c3').track({ sends: [a, [b, 4]] })`,
            ['$saw-1'],
            ['$saw-1@4'],
        ],
    ])('%s', (patch, toA, toB) => {
        const built = patchOf(`${BUSES}${patch}`);
        expect(busSends(built, 0)).toEqual(toA);
        expect(busSends(built, 1)).toEqual(toB);
    });

    test('every track sending to a bus is mixed into it', () => {
        const built = patchOf(
            `${BUSES}lead: $saw('c3').track({ sends: a })\nbass$1m: $sine('c4').track({ sends: [a, 2] })`,
        );
        expect(busSends(built, 0)).toEqual(['$saw-1', '$sine-1@2']);
    });

    test.each([
        [`lead: $saw('c3').track({ sends: 3 })`],
        [`lead: $saw('c3').track({ sends: [a, 2, 3] })`],
        [`lead: $saw('c3').track({ sends: [[a, 2], 3] })`],
        [`kick: $sine('c1')\nlead: $saw('c3').track({ sends: kick })`],
    ])('%s is rejected', (patch) => {
        expect(() => patchOf(`${BUSES}${patch}`)).toThrow(/sends/);
    });
});
