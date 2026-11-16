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

function vuLabels(patch: ReturnType<typeof run>['patch']): string[] {
    const meters = patch.vuMeters as unknown as { key: string }[];
    return meters.map((m) => m.key).filter((k) => k !== '__main__');
}

describe('$bus outs', () => {
    test('bus.out() routes the callback result like a collection out', () => {
        const { patch } = run(
            `const a = $bus((e) => e.lpf('1000hz')).out({ label: 'fx' })
             $saw('c3').out({ label: 'dry', sends: a })`,
        );
        expect(patch.modules.map((m) => m.moduleType)).toContain('$lpf');
        expect(vuLabels(patch)).toEqual(['fx', 'dry']);
    });

    test('bus.outMono() takes the collection outMono overloads', () => {
        expect(() =>
            run(
                `const a = $bus((e) => e).outMono(2, { label: 'fx' })
                 $saw('c3').out({ sends: a })`,
            ),
        ).not.toThrow();
    });

    test('a bus without .out() stays silent', () => {
        const { patch } = run(`const a = $bus((e) => e.lpf('1000hz'))
                               $saw('c3').out({ label: 'dry', sends: a })`);
        expect(vuLabels(patch)).toEqual(['dry']);
    });

    test('bus.out() needs the callback to return at least one lane', () => {
        expect(() => run(`$bus(() => []).out()`)).toThrow(/returned no signal/);
    });

    test('bus.out() inside a $bus callback is an error', () => {
        expect(() =>
            run(`const b = $bus((e) => e).out()
                 $saw('c3').out({ sends: b })
                 const a = $bus((e) => { b.out(); return e })
                 $saw('e3').out({ sends: a })`),
        ).toThrow(/on a bus is not allowed in \$bus callbacks/);
    });

    test('.out() on a collection inside a $bus callback still plays', () => {
        const { patch } =
            run(`const a = $bus((e) => { e.out({ label: 'old' }) })
                               $saw('c3').out({ label: 'dry', sends: a })`);
        expect(vuLabels(patch)).toEqual(['dry', 'old']);
    });

    test('bus.out() needs the callback to return a signal', () => {
        expect(() =>
            run(`const a = $bus((e) => { e.lpf('1000hz') }).out()
                 $saw('c3').out({ sends: a })`),
        ).toThrow(/must return a signal/);
    });

    test.each([
        [`$bus((e) => e).out({ sends: a })`],
        [`$bus((e) => e).outMono(1, { sends: [a, 2] })`],
    ])('%s cannot send to another bus', (out) => {
        expect(() =>
            run(`const a = $bus((e) => e).out()
                 ${out}`),
        ).toThrow(/a bus cannot send to another bus/);
    });

    test('bus.out() labels must be unique across outs', () => {
        expect(() =>
            run(`$bus((e) => e).out({ label: 'x' })
                 $saw('c3').out({ label: 'x' })`),
        ).toThrow(/must be unique/);
    });
});
