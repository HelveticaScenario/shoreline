/**
 * Track labels at run time: a top-level `name: expr` becomes a mixer track
 * with its mute/solo state and channels read off the label, `.track()` sets
 * its mix, and the label's name reads as the track for the rest of the patch.
 */

import { describe, expect, test } from 'vitest';
import type { ModuleSpec, PatchGraph } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import type { VuMeterDef } from '../../../shared/dsl/vuMeterTypes';
import { type DSLExecutionResult, executePatchScript } from '../executor';

function exec(source: string): DSLExecutionResult {
    return executePatchScript(source, schemas, {
        sampleRate: 48_000,
        workspaceRoot: '/workspace',
    });
}

function meters(patch: PatchGraph): VuMeterDef[] {
    return (patch.vuMeters as unknown as VuMeterDef[]).filter((m) => !m.main);
}

function moduleById(patch: PatchGraph, id: string): ModuleSpec {
    const module = patch.modules.find((m) => m.id === id);
    expect(module, `module ${id} should exist`).toBeDefined();
    return module!;
}

function gateOf(patch: PatchGraph, key: string): unknown {
    return moduleById(patch, `__vuMute_${key}`).params.value;
}

describe('track labels', () => {
    test('a label declares a stereo track on channel 0', () => {
        const { patch } = exec(`lead: $sine('c4')`);
        expect(meters(patch)).toEqual([
            expect.objectContaining({
                baseChannel: 0,
                channels: 2,
                key: 'lead',
                label: 'lead',
                mute: false,
                solo: false,
                sourceLocation: { column: 5, line: 1 },
            }),
        ]);
    });

    test('the suffix sets the stereo base channel or the mono channel', () => {
        const { patch } = exec(`
pad$2: $sine('c4')
click$5m: $noise()
`);
        expect(meters(patch)).toMatchObject([
            { baseChannel: 2, channels: 2, key: 'pad' },
            { baseChannel: 5, channels: 1, key: 'click' },
        ]);
    });

    test('underscores in the name show as spaces in the mixer', () => {
        const { patch } = exec(`bass_line: $saw('c2')`);
        expect(meters(patch)[0]).toMatchObject({
            key: 'bass_line',
            label: 'bass line',
        });
    });

    test('_ mutes a track and s_ or S_ solos it', () => {
        const { patch } = exec(`
_a: $sine('c4')
s_b: $sine('e4')
S_c: $sine('g4')
d: $sine('b4')
`);
        expect(meters(patch)).toMatchObject([
            { key: 'a', mute: true, solo: false },
            { key: 'b', mute: false, solo: true },
            { key: 'c', mute: false, solo: true },
            { key: 'd', mute: false, solo: false },
        ]);
        expect(gateOf(patch, 'a')).toBe(0);
        expect(gateOf(patch, 'b')).toBe(5);
        expect(gateOf(patch, 'c')).toBe(5);
        expect(gateOf(patch, 'd')).toBe(0);
    });

    test('muting keeps the hidden module ids, which follow the name', () => {
        const ids = (source: string) =>
            exec(source)
                .patch.modules.map((m) => m.id)
                .filter((id) => id.startsWith('__vu'))
                .sort();
        expect(ids(`_lead: $sine('c4')`)).toEqual(ids(`lead: $sine('c4')`));
    });

    test('an unlabelled expression is not heard', () => {
        const { patch } = exec(`$sine('c4')`);
        expect(meters(patch)).toEqual([]);
    });

    test('.track sets gain, pan and spread', () => {
        const { patch } = exec(
            `lead: $sine(['c4', 'e4']).track({ gain: 3, pan: -2, spread: 1 })`,
        );
        expect(meters(patch)[0]).toMatchObject({ gain: 3, pan: -2 });
        const pan = patch.modules.find((m) => m.moduleType === '$pan');
        expect(pan?.params.spread).toBe(1);
    });

    test('.track on an unlabelled expression is not heard', () => {
        const { patch } = exec(`$sine('c4').track({ gain: 3 })`);
        expect(meters(patch)).toEqual([]);
    });

    test('.track rejects options the label sets', () => {
        expect(() => exec(`a: $sine('c4').track({ mute: true })`)).toThrow(
            /no `mute` option/,
        );
    });

    test('a mono track takes no pan', () => {
        expect(() => exec(`a$0m: $sine('c4').track({ pan: 1 })`)).toThrow(
            /mono/,
        );
    });

    test('.track().scope() scopes the mixdown and returns the track', () => {
        const { patch } = exec(
            `lead: $sine(['c4', 'e4']).track({ pan: 1 }).scope().scope()`,
        );
        expect(patch.scopes).toHaveLength(2);
        expect(patch.scopes[0].lanes[0]).toMatchObject({
            moduleId: '__vuTap_lead',
        });
    });

    test('labels keep every call site where the patch wrote it', () => {
        const { patch } = exec(`lead: $sine('c4').scope()
bass$2: $saw('c2').scope()`);
        expect(
            patch.scopes.map(
                (s) => (s as { sourceLocation?: unknown }).sourceLocation,
            ),
        ).toEqual([
            { column: 23, line: 1 },
            { column: 20, line: 2 },
        ]);
    });
});

describe('track label errors', () => {
    test.each([
        ['__x: 1', /not a track label/],
        ['a-b: 1', /./],
        ['x$63: 1', /channel must be 0-62/],
        ['function f() { inner: 1 }', /top level/],
        ['a: { }', /goes before a signal/],
        ['a: 1, 2', /one expression/],
        ['a: 1\na$2: 2', /already a track named a/],
        ['const a = 1\na: 2', /already declared/],
        ['Math: 1', /DSL global/],
        ['_s_x: 1', /cannot start with s_/],
        ['S_S_x: 1', /cannot start with s_/],
        ['arguments: 1', /cannot name a track/],
        ['let: 1', /cannot name a track/],
        ['if (1) { var a = 1 }\na: 2', /already declared/],
        ['for (var a of [1]) {}\na: 2', /already declared/],
        ['$sine: 1', /DSL global/],
    ])('%s', (source, message) => {
        expect(() => exec(source)).toThrow(message);
    });

    test('errors name the line', () => {
        expect(() => exec(`a: $sine('c4')\n__b: 1`)).toThrow(/line 2/);
    });
});

describe('track bindings', () => {
    test("a track's name reads as its final signal", () => {
        const { patch } = exec(`
kick: $sine('c1')
bass: $saw('c2').amp(kick)
`);
        const cable = (id: string) => ({
            lane: 0,
            module: id,
            port: 'output',
            type: 'cable',
        });
        const kickGate = patch.modules.find(
            (m) =>
                m.moduleType === '$amp' &&
                JSON.stringify(m.params.input) ===
                    JSON.stringify([cable('__vuTap_kick')]),
        )!;
        const sidechain = patch.modules.find(
            (m) =>
                m.moduleType === '$amp' &&
                JSON.stringify(m.params.factor) ===
                    JSON.stringify([cable(kickGate.id)]),
        );
        expect(sidechain).toBeDefined();
    });

    test('a name can be read before its label', () => {
        expect(() =>
            exec(`
bass: $saw('c2').amp(kick)
kick: $sine('c1')
`),
        ).not.toThrow();
    });

    test("a bus track's name takes sends", () => {
        const { patch } = exec(`
verb: $bus().lpf('c5').track({ gain: 3 })
lead: $saw('c3').track({ sends: [verb, 2] })
`);
        expect(meters(patch).map((m) => m.key)).toEqual(['verb', 'lead']);
        expect(patch.modules.some((m) => m.moduleType === '$lpf')).toBe(true);
    });

    test('a send reaches a bus whose label comes later', () => {
        const { patch } = exec(`
lead: $saw('c3').track({ sends: [verb, 2] })
verb: $bus().lpf('c5')
`);
        expect(meters(patch).map((m) => m.key)).toEqual(['lead', 'verb']);
        expect(patch.modules.some((m) => m.moduleType === '$lpf')).toBe(true);
    });

    test('a send to a later track that is not a bus is an error', () => {
        expect(() =>
            exec(`
lead: $saw('c3').track({ sends: kick })
kick: $sine('c1')
`),
        ).toThrow(/is not a bus/);
    });

    test('a send to anything but a bus is an error', () => {
        expect(() =>
            exec(`
kick: $sine('c1')
lead: $saw('c3').track({ sends: kick })
`),
        ).toThrow(/sends/);
    });

    test('a name cannot be assigned', () => {
        expect(() => exec(`kick: $sine('c1')\nkick = 1`)).toThrow();
    });
});
