import { describe, expect, test } from 'vitest';

import { formatControlQuickFix } from '../controlFixFormat';
import type { ControlQuickFix } from '../controlQuickFix';
import { apply, fixesAt, wrapFix } from './controlQuickFixHarness';

/** The source after applying the single wrap fix at the `|` cursor. */
function wrapped(text: string): string {
    const { source, fixes } = fixesAt(text);
    return apply(source, wrapFix(fixes));
}

function unwrapFix(fixes: ControlQuickFix[]): ControlQuickFix | undefined {
    return fixes.find((f) => f.kind === 'unwrap');
}

describe('computeControlQuickFixes — a control replaces a single-value $c/$signal', () => {
    test('a note in $c becomes a note slider for the param it sets', () => {
        const { fixes } = fixesAt(`$saw($c('a|3')).out();`);
        expect(wrapFix(fixes).title).toBe(
            `Replace $c('a3') with a note $slider`,
        );
        expect(wrapped(`$saw($c('a|3')).out();`)).toBe(
            `$saw($slider('freq', 'a3', 'a1', 'a5')).out();`,
        );
    });

    test('$signal is replaced the same way', () => {
        expect(wrapped(`$saw($signal('a|3')).out();`)).toBe(
            `$saw($slider('freq', 'a3', 'a1', 'a5')).out();`,
        );
        // A note without an octave reads as octave 4 and is kept as written.
        expect(wrapped(`$saw($signal('|c')).out();`)).toBe(
            `$saw($slider('freq', 'c', 'c2', 'c6')).out();`,
        );
    });

    test('an option member takes its param range', () => {
        expect(wrapped(`$pulse('a3', { width: $c(2.|5) }).out();`)).toBe(
            `$pulse('a3', { width: $slider('width', 2.5, 0, 5) }).out();`,
        );
    });

    test('the cursor on the option key or the $c name counts', () => {
        expect(wrapped(`$pulse('a3', { wid|th: $signal(2.5) }).out();`)).toBe(
            `$pulse('a3', { width: $slider('width', 2.5, 0, 5) }).out();`,
        );
        expect(wrapped(`$saw($|c('a3')).out();`)).toBe(
            `$saw($slider('freq', 'a3', 'a1', 'a5')).out();`,
        );
    });

    test('a gate param gets a toggle set from the value, or a button', () => {
        const { source, fixes } = fixesAt(`$adsr($c(|5)).out();`);
        expect(
            fixes
                .filter((f) => f.kind === 'wrap')
                .map((f) => [f.title, apply(source, f)]),
        ).toEqual([
            [
                `Replace $c(5) with $toggleBtn`,
                `$adsr($toggleBtn('gate', true)).out();`,
            ],
            [`Replace $c(5) with $btn`, `$adsr($btn('gate')).out();`],
        ]);
    });

    test('a standalone call is named after its variable', () => {
        expect(wrapped(`const root = $signal('c|3');`)).toBe(
            `const root = $slider('root', 'c3', 'c1', 'c5');`,
        );
        expect(wrapped(`const lvl = $c(0.|5);`)).toBe(
            `const lvl = $slider('lvl', 0.5, 0, 5);`,
        );
        expect(wrapped(`const lvl = $signal(0.|5);`)).toBe(
            `const lvl = $slider('lvl', 0.5, 0, 5);`,
        );
    });

    test('a ranged $c keeps the control inside it', () => {
        // A slider's .range takes no input range, so it cannot stand in.
        expect(wrapped(`$sine($c(|2).range(100, 200, 0, 10)).out();`)).toBe(
            `$sine($c($slider('args', 2, 0, 5)).range(100, 200, 0, 10)).out();`,
        );
        const ranged = fixesAt(
            `const lfo = $signal(|2);\n$sine(lfo.range(100, 200, 0, 10)).out();`,
        );
        for (const f of ranged.fixes) {
            expect(apply(ranged.source, f)).toContain('const lfo = $signal(');
        }
    });

    test('a patch with its own $c keeps the call', () => {
        const { source, fixes } = fixesAt(
            `const $c = (x) => x * 2;\n$saw($c(|2)).out();`,
        );
        for (const f of fixes) {
            expect(apply(source, f)).toContain('$c(');
        }
    });

    test('a slot no control drives keeps the call as it was', () => {
        const { source, fixes } = fixesAt(`$lpf($c(|1), '1000hz').out();`);
        for (const f of fixes) {
            expect(apply(source, f)).toContain('$lpf($c(');
        }
    });

    test('the title shows the literal without spacing or comments', () => {
        const { fixes } = fixesAt(`$saw($c( 'a|3' )).out();`);
        expect(wrapFix(fixes).title).toBe(
            `Replace $c('a3') with a note $slider`,
        );
    });

    test('a $c of several values keeps the control inside it', () => {
        expect(wrapped(`$saw($c('a|3', 'c4')).out();`)).toBe(
            `$saw($c($slider('args', 'a3', 'a1', 'a5'), 'c4')).out();`,
        );
    });
});

describe('computeControlQuickFixes — a slider back to $c', () => {
    test('replaces the slider with a $c of its value as written', () => {
        for (const [text, out] of [
            [
                `$saw($slider('f', 'a3', 'a1', |'a5')).out();`,
                `$saw($c('a3')).out();`,
            ],
            [
                `$pulse('a3', { width: $sli|der('w', 2.5, 0, 5) }).out();`,
                `$pulse('a3', { width: $c(2.5) }).out();`,
            ],
            [
                `const g = $cGroup('g');\n$sine($slider('p', '440hz', '110hz', '1760hz', g)|).out();`,
                `const g = $cGroup('g');\n$sine($c('440hz')).out();`,
            ],
        ]) {
            const { source, fixes } = fixesAt(text);
            const fix = unwrapFix(fixes)!;
            expect(apply(source, fix)).toBe(out);
        }
        const { fixes } = fixesAt(
            `$saw($slider('f', 'a3', 'a1', |'a5')).out();`,
        );
        expect(unwrapFix(fixes)!.title).toBe(`Replace $slider with $c('a3')`);
    });

    test('is not offered for a ranged slider or a patch with its own $c', () => {
        for (const text of [
            `$sine($slider('f', 2, 0, |5).range(100, 200)).out();`,
            `const v = $slider('f', 2, 0, |5);\n$sine(v.range(1, 2)).out();`,
            `const $c = (x) => x;\n$sine($slider('f', 2, 0, |5)).out();`,
        ]) {
            expect(unwrapFix(fixesAt(text).fixes), text).toBe(undefined);
        }
    });

    test('is not offered outside a slider', () => {
        expect(unwrapFix(fixesAt(`$saw($btn('|x')).out();`).fixes)).toBe(
            undefined,
        );
    });

    test('round-trips with the wrap fix', async () => {
        const { source, fixes } = fixesAt(
            `$saw($slider('freq', 'a3', 'a1', 'a5')|).out();`,
        );
        const unwrapped = apply(
            source,
            await formatControlQuickFix(source, unwrapFix(fixes)!, {
                singleQuote: true,
            }),
        );
        expect(unwrapped).toBe(`$saw($c('a3')).out();`);
        const at = unwrapped.indexOf('a3') + 1;
        expect(
            wrapped(unwrapped.slice(0, at) + '|' + unwrapped.slice(at)),
        ).toBe(source);
    });
});
