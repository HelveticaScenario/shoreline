/**
 * A track's `sends` feeds the track's final signal, after its gain, pan,
 * mute and solo, to each bus named, at full level or at a pair's linear level.
 * Every track feeds every bus, at 0 where its sends name none.
 */

import { describe, expect, test } from 'vitest';
import type { ModuleSpec, PatchGraph } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import type { VuMeterDef } from '../../../shared/dsl/vuMeterTypes';
import { executePatchScript } from '../executor';

const OPTIONS = { sampleRate: 48_000, workspaceRoot: '/workspace' } as const;

type Cable = { module: string; type: 'cable' };

function patchOf(source: string): PatchGraph {
    return executePatchScript(source, schemas, OPTIONS).patch;
}

/**
 * What bus `a` (index 0) or `b` (index 1) mixes, as `stages` or
 * `stages@level` per send. `stages` names each mixer stage the send passed
 * through, its own track first and then each group it plays into, as
 * `kick>drums>kit`; each stage is a gain gated by that track's mute and solo.
 * Each bus is found from the filter chained on it.
 */
function busSends(patch: PatchGraph, index: number): string[] {
    const byId = new Map(patch.modules.map((m) => [m.id, m]));
    const filter = patch.modules.find(
        (m) => m.moduleType === (index === 0 ? '$lpf' : '$hpf'),
    )!;
    const source = (cables: unknown) =>
        byId.get((cables as Cable[])[0].module)!;
    /** The track whose mute gate `module` is, if it is one. */
    const gateOf = (module: ModuleSpec) => {
        if (module.moduleType !== '$amp') {
            return null;
        }
        const cables = module.params.factor;
        if (!Array.isArray(cables) || cables[0]?.type !== 'cable') {
            return null;
        }
        const factor = source(cables).id;
        return factor.startsWith('__vuSlew_')
            ? factor.slice('__vuSlew_'.length)
            : null;
    };
    const stages = (id: string) => {
        const names: string[] = [];
        let module = byId.get(id)!;
        for (let name = gateOf(module); name !== null; name = gateOf(module)) {
            names.push(name);
            module = source(module.params.input);
            while (!module.id.startsWith('__vuTap_') && !gateOf(module)) {
                module = source(module.params.input);
            }
        }
        return names.length > 0 ? names.reverse().join('>') : id;
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
        if (from.moduleType !== '$amp' || gateOf(from) !== null) {
            return stages(from.id);
        }
        const { input: sent, factor } = from.params as {
            input: Cable[];
            factor: unknown;
        };
        // A numeric level is a dial's _smoothControl, shown by its value.
        const control = Array.isArray(factor)
            ? byId.get((factor as Cable[])[0]?.module)
            : undefined;
        const level =
            control?.moduleType === '_smoothControl'
                ? control.params.value
                : factor;
        return `${stages(sent[0].module)}@${JSON.stringify(level)}`;
    });
}

const BUSES = `a$2m: $bus().lpf('c8')\nb$3m: $bus().hpf('c0')\n`;

describe('track sends', () => {
    test.each([
        [`lead: $saw('c3').track({ sends: a })`, ['lead@5'], ['lead@0']],
        [`lead: $saw('c3').track({ sends: [a, 3] })`, ['lead@3'], ['lead@0']],
        [`lead: $saw('c3').track({ sends: [a, b] })`, ['lead@5'], ['lead@5']],
        [
            `lead: $saw('c3').track({ sends: [[a, 2], [b, 3]] })`,
            ['lead@2'],
            ['lead@3'],
        ],
        [
            `lead: $saw('c3').track({ sends: [a, [b, 4]] })`,
            ['lead@5'],
            ['lead@4'],
        ],
        [`lead: $saw('c3').track({ sends: [[a, 2]] })`, ['lead@2'], ['lead@0']],
        [
            `lead: $saw('c3').track({ sends: [a, [1, 2]] })`,
            ['lead@[1,2]'],
            ['lead@0'],
        ],
        [
            `lead$0m: $saw('c3').track({ sends: [a, [b, 4]] })`,
            ['lead@5'],
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
        expect(busSends(built, 0)).toEqual(['lead@5', 'bass@2']);
    });

    test('a muted track sends nothing, its send passing through its mute', () => {
        const built = patchOf(`${BUSES}_lead: $saw('c3').track({ sends: a })`);
        expect(busSends(built, 0)).toEqual(['lead@5']);
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
    test('a group sends its final signal', () => {
        const built = patchOf(
            `${BUSES}drums: $group().track({ sends: [a, 2] })\nkick$drums: $sine('c1')`,
        );
        expect(busSends(built, 0)).toEqual(['drums@2', 'kick>drums@0']);
    });

    test("a member's send passes through its group's stage", () => {
        const built = patchOf(
            `${BUSES}drums: $group().lpf('c4').track({ gain: 2, pan: 1 })\nkick$drums: $sine('c1').track({ sends: a })`,
        );
        expect(busSends(built, 0)).toEqual(['drums@0', 'kick>drums@5']);
        // The group's own filter plays only on the group track.
        expect(
            built.modules.filter((m) => m.moduleType === '$lpf'),
        ).toHaveLength(2);
    });

    test('a send passes through every group up the chain', () => {
        const built = patchOf(
            `${BUSES}kit: $group()\ndrums$kit: $group()\nkick$drums: $sine('c1').track({ sends: [a, 3] })`,
        );
        expect(busSends(built, 0)).toEqual([
            'kit@0',
            'drums>kit@0',
            'kick>drums>kit@3',
        ]);
    });

    test("a muted group's members send through its closed gate", () => {
        const built = patchOf(
            `${BUSES}_drums: $group()\nkick$drums: $sine('c1').track({ sends: a })`,
        );
        expect(busSends(built, 0)).toEqual(['drums@0', 'kick>drums@5']);
        const gate = built.modules.find((m) => m.id === '__vuMute_drums')!;
        expect(gate.params.value).toBe(0);
    });
});

describe("the mixer's send dials", () => {
    const metersOf = (source: string) =>
        new Map(
            (patchOf(source).vuMeters as VuMeterDef[]).map((m) => [m.key, m]),
        );

    test('every track and group has a send to every bus', () => {
        const meters = metersOf(
            `${BUSES}drums: $group()\nkick$drums: $sine('c1').track({ sends: [a, 2] })\nlead: $saw('c3').track({ sends: b })`,
        );
        const levels = (key: string) =>
            meters.get(key)!.sends!.map((s) => [s.bus, s.value, s.locked]);
        expect(levels('kick')).toEqual([
            ['a', 2, false],
            ['b', 0, false],
        ]);
        expect(levels('lead')).toEqual([
            ['a', 0, false],
            ['b', 5, false],
        ]);
        expect(levels('drums')).toEqual([
            ['a', 0, false],
            ['b', 0, false],
        ]);
        expect(meters.get('a')!.sends).toBeUndefined();
        expect(meters.get('__main__')!.sends).toBeUndefined();
    });

    test("a dial drives its send's level", () => {
        const patch = patchOf(`${BUSES}lead: $saw('c3').track({ sends: a })`);
        const lead = (patch.vuMeters as VuMeterDef[]).find(
            (m) => m.key === 'lead',
        )!;
        const control = patch.modules.find(
            (m) => m.id === lead.sends![0].moduleId,
        )!;
        expect(control.moduleType).toBe('_smoothControl');
        expect(control.params.value).toBe(5);
    });

    test('a signal level locks its dial, which tracks the signal', () => {
        const lead = metersOf(
            `${BUSES}lfo: $sine(1)\nlead: $saw('c3').track({ sends: [[a, $sine(2)], [a, 1], b] })`,
        ).get('lead')!;
        expect(lead.sends).toEqual([
            { bus: 'a', locked: true, moduleId: null, value: null },
            {
                bus: 'b',
                locked: false,
                moduleId: expect.any(String),
                value: 5,
            },
        ]);
        const single = metersOf(
            `${BUSES}lead: $saw('c3').track({ sends: [a, $sine(2)] })`,
        ).get('lead')!;
        expect(single.sends![0]).toMatchObject({
            locked: true,
            sourceIndex: 0,
        });
        expect(single.controlSources).toEqual([
            expect.objectContaining({ portName: 'output' }),
        ]);
    });

    test("a level past the dial's span locks the dial and plays as written", () => {
        const patch = patchOf(
            `${BUSES}lead: $saw('c3').track({ sends: [[a, -2], [b, 7]] })`,
        );
        const lead = (patch.vuMeters as VuMeterDef[]).find(
            (m) => m.key === 'lead',
        )!;
        expect(lead.sends).toEqual([
            { bus: 'a', locked: true, moduleId: null, value: -2 },
            { bus: 'b', locked: true, moduleId: null, value: 7 },
        ]);
        expect(busSends(patch, 0)).toEqual(['lead@-2']);
        expect(busSends(patch, 1)).toEqual(['lead@7']);
    });

    test('without buses, tracks have no sends', () => {
        expect(metersOf(`lead: $saw('c3')`).get('lead')!.sends).toBeUndefined();
    });
});
