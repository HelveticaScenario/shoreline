/**
 * A bus's callback returns its signal, and labelling the bus plays that
 * signal as a track; the label's name reads as the bus.
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
    test('a labelled bus plays the callback result as a track', () => {
        const { patch } = run(
            `fx: $bus((e) => e.lpf('1000hz'))
             dry: $saw('c3').track({ sends: fx })`,
        );
        expect(patch.modules.map((m) => m.moduleType)).toContain('$lpf');
        expect(vuLabels(patch)).toEqual(['fx', 'dry']);
    });

    test('a bus track takes a mono channel and .track() options', () => {
        const { patch } = run(
            `fx$2m: $bus((e) => e).track({ gain: 3 })
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

    test('an unlabelled bus stays silent', () => {
        const { patch } = run(`const a = $bus((e) => e.lpf('1000hz'))
                               dry: $saw('c3').track({ sends: a })`);
        expect(vuLabels(patch)).toEqual(['dry']);
    });

    test('a bus held in a const plays once labelled', () => {
        const { patch } = run(`const a = $bus((e) => e.lpf('1000hz'))
                               verb: a
                               dry: $saw('c3').track({ sends: a })`);
        expect(vuLabels(patch)).toEqual(['verb', 'dry']);
    });

    test('a bus track needs the callback to return at least one lane', () => {
        expect(() => run(`fx: $bus(() => [])`)).toThrow(/returned no signal/);
    });

    test('a bus track needs the callback to return a signal', () => {
        expect(() =>
            run(`fx: $bus((e) => { e.lpf('1000hz') })
                 dry: $saw('c3').track({ sends: fx })`),
        ).toThrow(/must return the signal/);
    });

    test('a bus cannot send to another bus', () => {
        expect(() =>
            run(`a: $bus((e) => e)
                 b: $bus((e) => e).track({ sends: a })`),
        ).toThrow(/a bus cannot send to another bus/);
    });

    test('bus names are track names, so they are unique', () => {
        expect(() =>
            run(`x: $bus((e) => e)
                 _x$2: $saw('c3')`),
        ).toThrow(/already a track named x/);
    });
});
