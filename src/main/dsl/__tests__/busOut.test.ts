/**
 * A bus's callback returns its signal, and `.out()`/`.outMono()` on the bus
 * route that signal like a collection's out.
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

function vuLabels(source: string): string[] {
    const meters = run(source).patch.vuMeters as unknown as { key: string }[];
    return meters.map((m) => m.key).filter((k) => k !== '__main__');
}

describe('$bus outs', () => {
    test('bus.out() routes the callback result like a collection out', () => {
        const viaBus = run(
            `const a = $bus((e) => e.lpf('1000hz')).out({ label: 'fx' })
             $saw('c3').send(a).out({ label: 'dry' })`,
        ).patch.modules.map((m) => m.moduleType);
        expect(viaBus).toContain('$lpf');
        expect(
            vuLabels(
                `const a = $bus((e) => e.lpf('1000hz')).out({ label: 'fx' })
                 $saw('c3').send(a).out({ label: 'dry' })`,
            ),
        ).toEqual(['fx', 'dry']);
    });

    test('bus.outMono() takes the collection outMono overloads', () => {
        expect(() =>
            run(
                `const a = $bus((e) => e).outMono(2, { label: 'fx' })
                 $saw('c3').send(a).out()`,
            ),
        ).not.toThrow();
    });

    test('a bus without .out() stays silent', () => {
        expect(
            vuLabels(`const a = $bus((e) => e.lpf('1000hz'))
                      $saw('c3').send(a).out({ label: 'dry' })`),
        ).toEqual(['dry']);
    });

    test('a bus returning no lanes adds no out', () => {
        expect(
            vuLabels(`$bus(() => []).out({ label: 'fx' })
                      $saw('c3').out({ label: 'dry' })`),
        ).toEqual(['dry']);
    });

    test('bus.out() needs the callback to return a signal', () => {
        expect(() =>
            run(`const a = $bus((e) => { e.lpf('1000hz') }).out()
                 $saw('c3').send(a)`),
        ).toThrow(/must return a signal/);
    });

    test('bus.out() labels must be unique across outs', () => {
        expect(() =>
            run(`$bus((e) => e).out({ label: 'x' })
                 $saw('c3').out({ label: 'x' })`),
        ).toThrow(/must be unique/);
    });
});
