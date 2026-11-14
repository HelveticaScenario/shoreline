import { describe, expect, test } from 'vitest';

import { $, $arrange, MiniParseError, type PatternPayload } from '../index';
import { ALIGNMENT_MODES } from '../../alignmentModes';
import { patternPayload } from '../../patterns';

/** The `__kind` of each payload from the root inward. */
function kinds(payload: PatternPayload): string[] {
    const out: string[] = [payload.__kind ?? ''];
    let p: PatternPayload = payload;
    while ('pattern' in p) {
        p = p.pattern;
        out.push(p.__kind ?? '');
    }
    return out;
}

describe('.scale', () => {
    test('wraps the pattern in a ScalePattern payload', () => {
        const r = patternPayload($('0 2 4').scale('c(major)'));
        expect(r.__kind).toBe('ScalePattern');
        expect(r.scale).toBe('c(major)');
        expect(r.pattern.__kind).toBe('ParsedPattern');
        expect(r.argument_spans.length).toBe(1);
    });

    test('non-string scale throws', () => {
        expect(() => $('0').scale(123 as never)).toThrow(MiniParseError);
    });

    test('on a scale pattern it wraps again, so the outer scale wins', () => {
        const r = patternPayload(
            $('0 2').scale('c(maj)').trans('1').scale('d(min)'),
        );
        expect(kinds(r)).toEqual([
            'ScalePattern',
            'TransPattern',
            'ScalePattern',
            'ParsedPattern',
        ]);
        expect(r.scale).toBe('d(min)');
    });

    test('is on every pattern, including arrangements and wrappers', () => {
        expect(patternPayload($('0').fast(2).scale('c(maj)')).__kind).toBe(
            'ScalePattern',
        );
        expect(
            patternPayload($arrange([1, $('0')]).scale('c(maj)')).__kind,
        ).toBe('ScalePattern');
    });
});

describe('.trans', () => {
    test('appends a TransPattern with default mode "in"', () => {
        const r = patternPayload($('0 2 4').scale('c(maj)').trans('0 2'));
        expect(r.__kind).toBe('TransPattern');
        expect(r.mode).toBe('in');
        expect(r.rhs.source).toBe('0 2');
        expect(r.argument_spans.length).toBe(2);
    });

    test('bare .trans(x) is deep-equal to .trans.in(x)', () => {
        const base = $('0 2 4').scale('c(maj)');
        expect(patternPayload(base.trans('0 2'))).toEqual(
            patternPayload(base.trans.in('0 2')),
        );
    });

    test('all 7 modes are callable on .trans', () => {
        const base = $('0 1 2').scale('c(maj)');
        for (const mode of ALIGNMENT_MODES) {
            expect(patternPayload(base.trans[mode]('0')).mode).toBe(mode);
        }
    });

    test('survives every pattern method', () => {
        const base = $('0 1').scale('c(maj)');
        const chained = [
            base.fast(2),
            base.slow('2 4'),
            base.struct('x ~ x'),
            base.beat('0,3', 8),
            base.glide(0.1),
        ];
        for (const p of chained) {
            expect(patternPayload(p.trans('1')).__kind).toBe('TransPattern');
        }
    });

    test('.trans().fast().trans() nests in chain order', () => {
        const r = patternPayload(
            $('0 2').scale('c(maj)').trans('1').fast('2 4').trans.squeeze('3'),
        );
        expect(kinds(r)).toEqual([
            'TransPattern',
            'FastPattern',
            'TransPattern',
            'ScalePattern',
            'ParsedPattern',
        ]);
        // Degrees, first rhs, factor, last rhs: the engine's source order.
        expect(r.argument_spans.length).toBe(4);
    });

    test('an arrangement of scale patterns takes .trans', () => {
        const r = patternPayload(
            $arrange(
                [2, $('0 2').scale('c(maj)')],
                [2, $('0 2').scale('a(min)')],
            ).trans('<0 1>'),
        );
        expect(kinds(r)).toEqual(['TransPattern', 'ArrangePattern']);
    });

    test('$arrange(...).scale(...).trans(...).fast(...).trans(...)', () => {
        const r = patternPayload(
            $arrange([1, $('0')], [1, $('2')])
                .scale('c(maj)')
                .trans('1')
                .fast(2)
                .trans('2'),
        );
        expect(r.__kind).toBe('TransPattern');
    });

    test('a pattern that is not a scale pattern has no .trans', () => {
        const plain = $('c4') as unknown as { trans: unknown };
        expect(() => plain.trans).toThrow(/needs a scale pattern/);
        const mixed = $arrange(
            [1, $('c4')],
            [1, $('0').scale('c(maj)')],
        ) as unknown as { trans: unknown };
        expect(() => mixed.trans).toThrow(/needs a scale pattern/);
        expect('trans' in $('0').scale('c(maj)')).toBe(true);
    });

    test('chain is immutable', () => {
        const base = $('0').scale('c(maj)');
        base.trans('1');
        expect(patternPayload(base).__kind).toBe('ScalePattern');
    });

    test('empty or non-string RHS throws MiniParseError', () => {
        const base = $('0 2').scale('c(maj)');
        expect(() => base.trans('')).toThrow(MiniParseError);
        expect(() => base.trans('   ')).toThrow(MiniParseError);
        expect(() => base.trans(123 as never)).toThrow(MiniParseError);
        expect(() => base.trans.squeeze(null as never)).toThrow(MiniParseError);
    });

    test('RHS is parsed into its own AST', () => {
        const r = patternPayload($('0 2 4').scale('c(maj)').trans('1 3'));
        expect('Sequence' in r.rhs.ast).toBe(true);
        expect(r.rhs.all_spans.length).toBe(2);
    });
});

describe('scale pattern wire payload', () => {
    test('is plain data', () => {
        const r = patternPayload($('0 2').scale('c(maj)').trans('1').fast(2));
        expect(JSON.parse(JSON.stringify(r))).toEqual(r);
    });
});
