import { describe, expect, test } from 'vitest';

import { ADD_ALL_TITLE } from '../controlAddFixes';
import { formatControlQuickFix } from '../controlFixFormat';
import type { ControlQuickFix } from '../controlQuickFix';
import { apply, fixesAt } from './controlQuickFixHarness';

function addAll(text: string): {
    source: string;
    fix: ControlQuickFix | undefined;
} {
    const { source, fixes } = fixesAt(text);
    return { fix: fixes.find((f) => f.title === ADD_ALL_TITLE), source };
}

/** The text from the last added control's start to the caret. */
function controlAtCaret(source: string, fix: ControlQuickFix): string {
    return apply(source, fix).slice(
        fix.span.start + fix.controlOffset,
        fix.span.start + fix.caretOffset,
    );
}

describe('computeControlQuickFixes — add controls for all unset params', () => {
    test('creates the config object with a control for each unset param', () => {
        const { source, fix } = addAll(`$pulse('a3'|).out();`);
        expect(apply(source, fix!)).toBe(
            `$pulse('a3', { width: $slider('width', 2.5, 0, 5), pwm: $slider('pwm', 0, -5, 5), sync: $btn('sync'), phaseOffset: $slider('phaseOffset', 0, 0, 1) }).out();`,
        );
        expect(controlAtCaret(source, fix!)).toBe(
            `$slider('phaseOffset', 0, 0, 1)`,
        );
    });

    test('a gate gets a toggle and a trigger a button', () => {
        const { source, fix } = addAll(`$adsr(|).out();`);
        const out = apply(source, fix!);
        expect(out).toContain(`$adsr($toggleBtn('gate', false), {`);
        expect(out).toContain(`retrigger: $btn('retrigger')`);
    });

    test('fills the open positional slots, then the config object', () => {
        const { source, fix } = addAll(`$lpf($sine(0)|).out();`);
        expect(apply(source, fix!)).toBe(
            `$lpf($sine(0), $slider('cutoff', '260hz', '65hz', '1040hz'), $slider('resonance', 0, 0, 5)).out();`,
        );
    });

    test('appends to an existing config object, skipping set keys', () => {
        const { source, fix } = addAll(`$pulse('a3', { width: 2.5| }).out();`);
        const out = apply(source, fix!);
        expect(out).toContain(
            `pwm: $slider('pwm', 0, -5, 5), sync: $btn('sync'), phaseOffset: $slider('phaseOffset', 0, 0, 1)`,
        );
        expect(out.match(/width/g)).toHaveLength(1);
    });

    test('labels stay unique against existing controls', () => {
        const { source, fix } = addAll(
            `$slider('width', 1, 0, 5);\n$pulse('a3'|).out();`,
        );
        expect(apply(source, fix!)).toContain(
            `width: $slider('width 2', 2.5, 0, 5)`,
        );
    });

    test('nothing is added where a spread or computed key may set it', () => {
        for (const text of [
            `$pulse('a3', { ...cfg| }).out();`,
            `$pulse('a3', { [k]: 1| }).out();`,
            `$pulse('a3', { width: 1, get pwm() { return 0; }| }).out();`,
            `$pulse(...args|).out();`,
        ]) {
            const { fixes } = fixesAt(text);
            expect(fixes.filter((f) => f.kind !== 'wrap')).toEqual([]);
        }
    });

    test('is not offered when only one param is unset', () => {
        expect(addAll(`$sine(0).lpf('100hz'|).out();`).fix).toBeUndefined();
    });

    test('formats like Format Document', async () => {
        const { source, fix } = addAll(`$pulse('a3', { width: 2.5| }).out();`);
        const formatted = await formatControlQuickFix(source, fix!, {
            printWidth: 80,
            singleQuote: true,
            tabWidth: 2,
            trailingComma: 'all',
        });
        expect(apply(source, formatted)).toBe(
            [
                `$pulse('a3', {`,
                `  width: 2.5,`,
                `  pwm: $slider('pwm', 0, -5, 5),`,
                `  sync: $btn('sync'),`,
                `  phaseOffset: $slider('phaseOffset', 0, 0, 1),`,
                `}).out();`,
            ].join('\n'),
        );
        expect(controlAtCaret(source, formatted)).toBe(
            `$slider('phaseOffset', 0, 0, 1)`,
        );
    });
});
