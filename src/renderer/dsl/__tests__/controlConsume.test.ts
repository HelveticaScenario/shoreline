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
        const { fixes } = fixesAt(`out: $saw($c('a|3'));`);
        expect(wrapFix(fixes).title).toBe(
            `Replace $c('a3') with a note $slider`,
        );
        expect(wrapped(`out: $saw($c('a|3'));`)).toBe(
            `out: $saw($slider('freq', 'a3', 'a1', 'a5'));`,
        );
    });

    test('$signal is replaced the same way', () => {
        expect(wrapped(`out: $saw($signal('a|3'));`)).toBe(
            `out: $saw($slider('freq', 'a3', 'a1', 'a5'));`,
        );
        // A note without an octave reads as octave 4 and is kept as written.
        expect(wrapped(`out: $saw($signal('|c'));`)).toBe(
            `out: $saw($slider('freq', 'c', 'c2', 'c6'));`,
        );
    });

    test('an option member takes its param range', () => {
        expect(wrapped(`out: $pulse('a3', { width: $c(2.|5) });`)).toBe(
            `out: $pulse('a3', { width: $slider('width', 2.5, 0, 5) });`,
        );
    });

    test('the cursor on the option key or the $c name counts', () => {
        expect(wrapped(`out: $pulse('a3', { wid|th: $signal(2.5) });`)).toBe(
            `out: $pulse('a3', { width: $slider('width', 2.5, 0, 5) });`,
        );
        expect(wrapped(`out: $saw($|c('a3'));`)).toBe(
            `out: $saw($slider('freq', 'a3', 'a1', 'a5'));`,
        );
    });

    test('a gate param gets a toggle set from the value, or a button', () => {
        const { source, fixes } = fixesAt(`out: $adsr($c(|5));`);
        expect(
            fixes
                .filter((f) => f.kind === 'wrap')
                .map((f) => [f.title, apply(source, f)]),
        ).toEqual([
            [
                `Replace $c(5) with $toggleBtn`,
                `out: $adsr($toggleBtn('gate', true));`,
            ],
            [`Replace $c(5) with $btn`, `out: $adsr($btn('gate'));`],
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

    test('a patch with its own $c keeps the call', () => {
        const { source, fixes } = fixesAt(
            `const $c = (x) => x * 2;\nout: $saw($c(|2));`,
        );
        for (const f of fixes) {
            expect(apply(source, f)).toContain('$c(');
        }
    });

    test('a slot no control drives keeps the call as it was', () => {
        const { source, fixes } = fixesAt(`out: $lpf($c(|1), '1000hz');`);
        for (const f of fixes) {
            expect(apply(source, f)).toContain('$lpf($c(');
        }
    });

    test('the title shows the literal without spacing or comments', () => {
        const { fixes } = fixesAt(`out: $saw($c( 'a|3' ));`);
        expect(wrapFix(fixes).title).toBe(
            `Replace $c('a3') with a note $slider`,
        );
    });

    test('a $c of several values keeps the control inside it', () => {
        expect(wrapped(`out: $saw($c('a|3', 'c4'));`)).toBe(
            `out: $saw($c($slider('args', 'a3', 'a1', 'a5'), 'c4'));`,
        );
    });
});

describe('computeControlQuickFixes — a slider back to $c', () => {
    test('replaces the slider with a $c of its value as written', () => {
        for (const [text, out] of [
            [
                `out: $saw($slider('f', 'a3', 'a1', |'a5'));`,
                `out: $saw($c('a3'));`,
            ],
            [
                `out: $pulse('a3', { width: $sli|der('w', 2.5, 0, 5) });`,
                `out: $pulse('a3', { width: $c(2.5) });`,
            ],
            [
                `const g = $cGroup('g');\nout: $sine($slider('p', '440hz', '110hz', '1760hz', g)|);`,
                `const g = $cGroup('g');\nout: $sine($c('440hz'));`,
            ],
        ]) {
            const { source, fixes } = fixesAt(text);
            const fix = unwrapFix(fixes)!;
            expect(apply(source, fix)).toBe(out);
        }
        const { fixes } = fixesAt(
            `out: $saw($slider('f', 'a3', 'a1', |'a5'));`,
        );
        expect(unwrapFix(fixes)!.title).toBe(`Replace $slider with $c('a3')`);
    });

    test('swaps plainly, leaving any .range call as written', () => {
        const { source, fixes } = fixesAt(
            `out: $sine($slider('f', 2, 0, |5).range(100, 200));`,
        );
        expect(apply(source, unwrapFix(fixes)!)).toBe(
            `out: $sine($c(2).range(100, 200));`,
        );
    });

    test('is not offered in a patch with its own $c', () => {
        const text = `const $c = (x) => x;\nout: $sine($slider('f', 2, 0, |5));`;
        expect(unwrapFix(fixesAt(text).fixes)).toBe(undefined);
    });

    test('is not offered outside a slider', () => {
        expect(unwrapFix(fixesAt(`out: $saw($btn('|x'));`).fixes)).toBe(
            undefined,
        );
    });

    test('round-trips with the wrap fix', async () => {
        const { source, fixes } = fixesAt(
            `out: $saw($slider('freq', 'a3', 'a1', 'a5')|);`,
        );
        const unwrapped = apply(
            source,
            await formatControlQuickFix(source, unwrapFix(fixes)!, {
                singleQuote: true,
            }),
        );
        expect(unwrapped).toBe(`out: $saw($c('a3'));`);
        const at = unwrapped.indexOf('a3') + 1;
        expect(
            wrapped(unwrapped.slice(0, at) + '|' + unwrapped.slice(at)),
        ).toBe(source);
    });
});
