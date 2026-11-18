import { describe, expect, test } from 'vitest';

import { addFix, apply, fixesAt, wrapFix } from './controlQuickFixHarness';

/** The fixed source of the one wrap offered at the cursor. */
function wrapped(text: string): string {
    const { source, fixes } = fixesAt(text);
    return apply(source, wrapFix(fixes));
}

function wrapsAt(text: string): number {
    return fixesAt(text).fixes.filter((f) => f.kind === 'wrap').length;
}

describe('computeControlQuickFixes — literals inside arguments', () => {
    test('a poly array element takes its param control', () => {
        expect(wrapped(`out: $saw(['a3', 'c|4'])`)).toBe(
            `out: $saw(['a3', $slider('freq', 'c4', 'c2', 'c6')])`,
        );
        expect(wrapped(`$saw('a3').lpf(['200hz', '4|00hz'])`)).toContain(
            `$slider('cutoff', '400hz'`,
        );
        expect(wrapped(`out: $saw('a3', { shape: [1, 2.|5] })`)).toBe(
            `out: $saw('a3', { shape: [1, $slider('shape', 2.5, 0, 5)] })`,
        );
    });

    test('a poly array element of an API param takes its control', () => {
        expect(wrapped(`$phase.bend([1.|5, 2])`)).toBe(
            `$phase.bend([$slider('amount', 1.5, -5, 5), 2])`,
        );
        expect(wrapped(`out: $saw('a3').track({ gain: [1.|5, 2] })`)).toBe(
            `out: $saw('a3').track({ gain: [$slider('gain', 1.5, 0, 5), 2] })`,
        );
    });

    test('an element of a list of signals wraps', () => {
        expect(wrapped(`$mix([$saw('a3'), 1.|5])`)).toBe(
            `$mix([$saw('a3'), $slider('inputs', 1.5, 0, 5)])`,
        );
        expect(wrapped(`$step([1.|5, 2], $btn('next'))`)).toBe(
            `$step([$slider('steps', 1.5, 0, 5), 2], $btn('next'))`,
        );
    });

    test('only the signal positions of a tuple wrap', () => {
        expect(wrapped(`$keyframes([[1.|5, 0]])`)).toBe(
            `$keyframes([[$slider('keyframes', 1.5, 0, 5), 0]])`,
        );
        expect(wrapsAt(`$keyframes([[1.5, |0]])`)).toBe(0);
        const slice = `$sampler('x.wav', $btn('g'), { slice: [[0, 0.5], 1.|5] })`;
        expect(wrapped(slice)).toContain(
            `slice: [[0, 0.5], $slider('slice', 1.5, 0, 5)]`,
        );
        expect(
            wrapsAt(
                `$sampler('x.wav', $btn('g'), { slice: [[0, 0.|5], 1.5] })`,
            ),
        ).toBe(0);
    });

    test('a member of a nested object takes its declared control', () => {
        const eq = (member: string) =>
            `$eq($saw('a3'), { sections: [{ freq: '1000hz', ${member} }] })`;
        expect(wrapped(eq('gain: |3'))).toContain(
            `gain: $slider('gain', 3, -24, 24)`,
        );
        expect(wrapped(eq('q: |4'))).toContain(`q: $slider('q', 4, 0.1, 20)`);
        expect(
            wrapped(`$eq($saw('a3'), { sections: [{ freq: '10|00hz' }] })`),
        ).toContain(`freq: $slider('freq', '1000hz', '250hz', '4000hz')`);
    });

    test('a signal param with a non-signal alternative wraps', () => {
        expect(wrapped(`$quantizer($saw('a3'), 'c|4')`)).toContain(
            `$slider('scale', 'c4'`,
        );
        expect(
            wrapped(`$sampler('x.wav', $btn('g'), { slice: 1.|5 })`),
        ).toContain(`{ slice: $slider('slice', 1.5, 0, 5) }`);
    });

    test('a $c holding an element literal is replaced', () => {
        const { source, fixes } = fixesAt(`$mix([$saw('a3'), $c(1.|5)])`);
        const fix = wrapFix(fixes);
        expect(fix.title).toBe(`Replace $c(1.5) with $slider`);
        expect(apply(source, fix)).toBe(
            `$mix([$saw('a3'), $slider('inputs', 1.5, 0, 5)])`,
        );
    });

    test('an element after a spread has no known slot', () => {
        expect(wrapsAt(`out: $saw([...['a3'], 'c|4'])`)).toBe(0);
    });
});

describe('computeControlQuickFixes — required API params', () => {
    test('a missing required knob is added at 0 within its range', () => {
        const { source, fixes } = fixesAt(`$phase.bend(|)`);
        expect(apply(source, addFix(fixes, 'amount'))).toBe(
            `$phase.bend($slider('amount', 0, -5, 5))`,
        );
        const nested = fixesAt(
            `$wavetable('a3', 'x.wav', { phase: $phase.sync(|) })`,
        );
        expect(apply(nested.source, addFix(nested.fixes, 'ratio'))).toContain(
            `$phase.sync($slider('ratio', 0, 0, 5))`,
        );
    });

    test('a missing required knob starts at its stated default', () => {
        const { source, fixes } = fixesAt(`$phase.pwm(|)`);
        expect(apply(source, addFix(fixes, 'width'))).toBe(
            `$phase.pwm($slider('width', 2.5, 0, 5))`,
        );
    });

    test('a missing required signal with no stated range is never added', () => {
        for (const call of [
            `$v.layer(|)`,
            `$v.invert(|)`,
            `$scopeXY(|)`,
            `$sine('c4').range(|)`,
            `$v.osc(1).scale(|)`,
        ]) {
            expect(
                fixesAt(call).fixes.filter((f) => f.kind !== 'wrap'),
                call,
            ).toEqual([]);
        }
    });
});
