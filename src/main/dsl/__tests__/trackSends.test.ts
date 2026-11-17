/**
 * A track's `sends` feeds the track's final signal, after its gain, pan,
 * mute and solo, to each bus named, at full level or at a pair's gain.
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
 * What bus `a` (index 0) or `b` (index 1) mixes, as `track` or
 * `track@level` per send, where `track` names the track whose final output
 * (its gain stage gated by mute and solo) is sent. Each bus is found from the
 * filter chained on it.
 */
function busSends(patch: PatchGraph, index: number): string[] {
    const byId = new Map(patch.modules.map((m) => [m.id, m]));
    const filter = patch.modules.find(
        (m) => m.moduleType === (index === 0 ? '$lpf' : '$hpf'),
    )!;
    const source = (cables: unknown) =>
        byId.get((cables as Cable[])[0].module)!;
    const track = (id: string) => {
        const gated = byId.get(id)!;
        const tap = source(gated.params.input).id;
        return gated.moduleType === '$amp' && tap.startsWith('__vuTap_')
            ? tap.slice('__vuTap_'.length)
            : id;
    };
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
            return track(from.id);
        }
        const { input: gained, level } = from.params as {
            input: Cable[];
            level: unknown;
        };
        return `${track(gained[0].module)}@${JSON.stringify(level)}`;
    });
}

const BUSES = `a$2m: $bus().lpf('c8')\nb$3m: $bus().hpf('c0')\n`;

describe('track sends', () => {
    test.each([
        [`lead: $saw('c3').track({ sends: a })`, ['lead'], []],
        [`lead: $saw('c3').track({ sends: [a, 3] })`, ['lead@3'], []],
        [`lead: $saw('c3').track({ sends: [a, b] })`, ['lead'], ['lead']],
        [
            `lead: $saw('c3').track({ sends: [[a, 2], [b, 3]] })`,
            ['lead@2'],
            ['lead@3'],
        ],
        [
            `lead: $saw('c3').track({ sends: [a, [b, 4]] })`,
            ['lead'],
            ['lead@4'],
        ],
        [`lead: $saw('c3').track({ sends: [[a, 2]] })`, ['lead@2'], []],
        [`lead: $saw('c3').track({ sends: [a, [1, 2]] })`, ['lead@[1,2]'], []],
        [
            `lead$0m: $saw('c3').track({ sends: [a, [b, 4]] })`,
            ['lead'],
            ['lead@4'],
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
        expect(busSends(built, 0)).toEqual(['lead', 'bass@2']);
    });

    test('a muted track sends nothing, its send passing through its mute', () => {
        const built = patchOf(`${BUSES}_lead: $saw('c3').track({ sends: a })`);
        expect(busSends(built, 0)).toEqual(['lead']);
        const gate = built.modules.find((m) => m.id === '__vuMute_lead')!;
        expect(gate.params.value).toBe(0);
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

describe('sends from groups', () => {
    test('a group member and a group send their final signals', () => {
        const built = patchOf(
            `${BUSES}drums: $group().track({ sends: [a, 2] })\nkick$drums: $sine('c1').track({ sends: a })`,
        );
        expect(busSends(built, 0)).toEqual(['drums@2', 'kick']);
    });
});
