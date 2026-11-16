/**
 * `$group()` is the sum of the tracks whose labels name its track. A track
 * with a group suffix plays into the group and not to the output channels.
 */

import { describe, expect, test } from 'vitest';
import type { PatchGraph } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import type { VuMeterDef } from '../../../shared/dsl/vuMeterTypes';
import { executePatchScript } from '../executor';

function run(source: string): PatchGraph {
    return executePatchScript(source, schemas, {
        sampleRate: 48_000,
        workspaceRoot: '/workspace',
    }).patch;
}

const meters = (patch: PatchGraph) =>
    (patch.vuMeters as unknown as VuMeterDef[]).filter((m) => !m.main);

const gate = (patch: PatchGraph, key: string) =>
    patch.modules.find((m) => m.id === `__vuMute_${key}`)!.params.value;

/** The ids of the modules `id` reads, through cables in its params. */
function reads(patch: PatchGraph, id: string): Set<string> {
    const byId = new Map(patch.modules.map((m) => [m.id, m]));
    const seen = new Set<string>();
    const visit = (value: unknown): void => {
        if (Array.isArray(value)) {
            value.forEach(visit);
        } else if (typeof value === 'object' && value !== null) {
            const cable = value as { type?: string; module?: string };
            if (cable.type === 'cable' && cable.module) {
                if (!seen.has(cable.module)) {
                    seen.add(cable.module);
                    visit(byId.get(cable.module)?.params);
                }
                return;
            }
            Object.values(value).forEach(visit);
        }
    };
    visit(byId.get(id)?.params);
    return seen;
}

const DRUMS = `
kick$drums: $sine('c1')
snare$drums: $noise()
drums$2: $group().lpf('c8').track({ gain: 3 })
bass: $saw('c2')
`;

describe('track groups', () => {
    test('members play into the group, which plays to its channels', () => {
        const patch = run(DRUMS);
        expect(meters(patch)).toMatchObject([
            { group: 'drums', isGroup: false, key: 'kick' },
            { group: 'drums', isGroup: false, key: 'snare' },
            { baseChannel: 2, group: null, isGroup: true, key: 'drums' },
            { group: null, isGroup: false, key: 'bass' },
        ]);
        const drums = reads(patch, '__vuTap_drums');
        expect(drums.has('__vuTap_kick')).toBe(true);
        expect(drums.has('__vuSlew_kick')).toBe(true);
        expect(drums.has('__vuTap_snare')).toBe(true);
        expect(drums.has('__vuTap_bass')).toBe(false);
        // The members reach the output only through the group.
        const root = patch.modules.find((m) => m.id === 'ROOT_OUTPUT')!;
        const lanes = root.params.value as unknown[];
        expect(lanes).toHaveLength(4);
    });

    test('groups nest', () => {
        const patch = run(`
kick$drums: $sine('c1')
drums$rhythm: $group()
bass$rhythm: $saw('c2')
rhythm: $group().track({ gain: 4 })
`);
        const rhythm = reads(patch, '__vuTap_rhythm');
        expect(rhythm.has('__vuTap_drums')).toBe(true);
        expect(rhythm.has('__vuTap_kick')).toBe(true);
        expect(rhythm.has('__vuTap_bass')).toBe(true);
    });

    test('a group can be labelled before or after its members', () => {
        expect(() =>
            run(`drums: $group()\nkick$drums: $sine('c1')`),
        ).not.toThrow();
    });

    test('a group track sends, and members keep their sends', () => {
        expect(() =>
            run(`
verb: $bus().plate()
kick$drums: $sine('c1').track({ sends: verb })
drums: $group().track({ sends: [verb, 2] })
`),
        ).not.toThrow();
    });

    test("a group's name reads as its signal", () => {
        expect(() =>
            run(`${DRUMS}\npad: $sine('c4').amp($motionFollower(drums))`),
        ).not.toThrow();
    });

    test.each([
        [
            `kick$bass: $sine('c1')\nbass: $saw('c2')`,
            /bass, which is not a group/,
        ],
        [`kick$drums: $sine('c1')`, /no track named drums/],
        [`a$a: $group()`, /plays back into a/],
        [`a$b: $group()\nb$a: $group()`, /plays back into/],
        [
            `verb$fx: $bus().plate()\nfx: $group()`,
            /a bus cannot play into a group/,
        ],
        [
            `const g = $group()\none: g.lpf('c5')\ntwo: g.hpf('c3')`,
            /a group has one track/,
        ],
        [`drums: $group(1)`, /takes no arguments/],
        [`fx: $lpf($group(), 'c5')\nkick$fx: $sine('c1')`, /not a group/],
    ])('%s', (source, message) => {
        expect(() => run(source)).toThrow(message);
    });
});

describe('solo and mute across groups', () => {
    const states = (source: string) => {
        const patch = run(source);
        return Object.fromEntries(
            meters(patch).map((m) => [m.key, gate(patch, m.key) === 5]),
        );
    };
    const patch = (kick: string, snare: string, drums: string, bass: string) =>
        `${kick}kick$drums: $sine('c1')\n${snare}snare$drums: $noise()\n${drums}drums: $group()\n${bass}bass: $saw('c2')`;

    test('a soloed member plays through its group', () => {
        expect(states(patch('S_', '', '', ''))).toEqual({
            bass: false,
            drums: true,
            kick: true,
            snare: false,
        });
    });

    test('a soloed group plays with all its members', () => {
        expect(states(patch('', '', 'S_', ''))).toEqual({
            bass: false,
            drums: true,
            kick: true,
            snare: true,
        });
    });

    test('a muted member of a soloed group stays muted', () => {
        expect(states(patch('_', '', 'S_', ''))).toMatchObject({
            kick: false,
            snare: true,
        });
    });

    test('a muted and soloed group plays with its members', () => {
        expect(states(patch('', '', 'S__', ''))).toEqual({
            bass: false,
            drums: true,
            kick: true,
            snare: true,
        });
    });

    test('a muted group silences what plays through it', () => {
        expect(states(patch('', '', '_', ''))).toEqual({
            bass: true,
            drums: false,
            kick: true,
            snare: true,
        });
    });

    test('a solo outside the group silences it', () => {
        expect(states(patch('', '', '', 'S_'))).toEqual({
            bass: true,
            drums: false,
            kick: false,
            snare: false,
        });
    });
});
