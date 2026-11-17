/**
 * `$bus()` is the mix of what tracks send to it: a signal to chain on. The
 * track rooted at it is the bus's track, which sends reach by its name.
 */

import { describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';

function run(source: string) {
    return executePatchScript(source, schemas, {
        sampleRate: 48_000,
        workspaceRoot: '/workspace',
    });
}

function vuLabels(patch: ReturnType<typeof run>['patch']): string[] {
    const meters = patch.vuMeters as unknown as { key: string }[];
    return meters.map((m) => m.key).filter((k) => k !== '__main__');
}

describe('$bus tracks', () => {
    test('a track chained from $bus() plays what is sent to it', () => {
        const { patch } = run(
            `fx: $bus().lpf('1000hz')
             dry: $saw('c3').track({ sends: fx })`,
        );
        const lpf = patch.modules.find((m) => m.moduleType === '$lpf')!;
        const [input] = lpf.params.input as { module: string }[];
        const lanes = patch.modules.find((m) => m.id === input.module)!;
        expect(lanes.moduleType).toBe('$setLanes');
        expect(lanes.params.lanes).toBe(1);
        expect(vuLabels(patch)).toEqual(['fx', 'dry']);
        expect(
            (
                patch.vuMeters as unknown as { key: string; isBus?: boolean }[]
            ).map((m) => [m.key, m.isBus === true]),
        ).toEqual([
            ['fx', true],
            ['dry', false],
            ['__main__', false],
        ]);
    });

    test('a bare $bus() is a bus track', () => {
        const { patch } = run(
            `fx$2m: $bus()
             dry: $saw('c3').track({ sends: [fx, 2] })`,
        );
        expect(vuLabels(patch)).toEqual(['fx', 'dry']);
    });

    test('a bus track takes .track() options and a mono channel', () => {
        const { patch } = run(
            `fx$2m: $bus().lpf('c5').track({ gain: 3 })
             dry: $saw('c3').track({ sends: fx })`,
        );
        const fx = (
            patch.vuMeters as unknown as {
                key: string;
                channels: number;
                baseChannel: number;
                gain: number;
            }[]
        ).find((m) => m.key === 'fx');
        expect(fx).toMatchObject({ baseChannel: 2, channels: 1, gain: 3 });
    });

    test('a bus held in a const is a bus once labelled', () => {
        const { patch } = run(`const b = $bus()
                               verb: b.lpf('1000hz').wet(2)
                               dry: $saw('c3').track({ sends: verb })`);
        expect(vuLabels(patch)).toEqual(['verb', 'dry']);
    });

    test('pipe and pipeMix keep a chain rooted at its bus', () => {
        expect(() =>
            run(`a: $bus().pipe((s) => s.lpf('c5'))
                 b: $bus().pipeMix((s) => s.lpf('c5'), 2)
                 dry: $saw('c3').track({ sends: [a, b] })`),
        ).not.toThrow();
    });

    test('a bus name is also its track signal', () => {
        expect(() =>
            run(`verb: $bus().lpf('c5')
                 dry: $saw('c3').amp(verb.active).track({ sends: verb })
                 follow: $sine('c4').amp($motionFollower(verb))`),
        ).not.toThrow();
    });

    test('a bus nothing sends to is silent', () => {
        const { patch } = run(`fx: $bus().lpf('c5')`);
        expect(vuLabels(patch)).toEqual(['fx']);
    });

    test('a signal that only reads a bus is not a bus', () => {
        expect(() =>
            run(`const b = $bus()
                 fx: $lpf(b, 'c5')
                 dry: $saw('c3').track({ sends: fx })`),
        ).toThrow(/sends cannot reach fx, which is not a bus/);
    });

    test('a ranged chain result is still rooted at its bus', () => {
        expect(() =>
            run(`verb: $bus().reverb()
                 dry: $saw('c3').track({ sends: verb })`),
        ).not.toThrow();
    });

    test('an output picked from a module with several is not a bus', () => {
        expect(() =>
            run(`low: $bus().xover()._low
                 dry: $saw('c3').track({ sends: low })`),
        ).toThrow(/which is not a bus/);
        expect(() =>
            run(`const b = $bus()
                 verb: b.plate()
                 env: b.motionFollower()
                 dry: $saw('c3').track({ sends: verb })`),
        ).not.toThrow();
    });

    test('a second track name in a pair is a send, not a level', () => {
        expect(() =>
            run(`verb: $bus().plate()
                 lead: $sine('c4')
                 dry: $saw('c3').track({ sends: [verb, lead] })`),
        ).toThrow(/sends cannot reach lead, which is not a bus/);
        expect(() =>
            run(`verb: $bus().plate()
                 lead: $sine('c4')
                 dry: $saw('c3').track({ sends: [verb, $c(lead)] })`),
        ).not.toThrow();
    });

    test('a bus has one track', () => {
        expect(() =>
            run(`const b = $bus()
                 one: b.lpf('c5')
                 two: b.hpf('c3')`),
        ).toThrow(/a bus has one track/);
    });

    test('a bus cannot send to another bus', () => {
        expect(() =>
            run(`a: $bus()
                 b: $bus().track({ sends: a })`),
        ).toThrow(/a bus cannot send to another bus/);
    });

    test('a bus cannot be soloed', () => {
        expect(() => run(`S_fx: $bus()`)).toThrow(/a bus cannot be soloed/);
        expect(() => run(`_S_fx: $bus().lpf('c5')`)).toThrow(
            /a bus cannot be soloed/,
        );
    });

    test("a bus plays through other tracks' solos, unless muted", () => {
        const gate = (source: string, key: string) =>
            run(source).patch.modules.find((m) => m.id === `__vuMute_${key}`)!
                .params.value;
        const patch = (fx: string) =>
            `${fx}fx: $bus().lpf('c5')
             S_lead: $saw('c3').track({ sends: fx })
             bass: $saw('c2')`;
        expect(gate(patch(''), 'fx')).toBe(5);
        expect(gate(patch(''), 'bass')).toBe(0);
        expect(gate(patch('_'), 'fx')).toBe(0);
    });

    test('$bus takes no callback', () => {
        expect(() => run(`fx: $bus((m) => m)`)).toThrow(/takes no arguments/);
    });
});
