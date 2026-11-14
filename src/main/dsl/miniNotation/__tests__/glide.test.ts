import { isKind } from './kinds';
import { describe, expect, test } from 'vitest';

import { $, $arrange, MiniParseError } from '../index';
import { patternPayload } from '../../patterns';

describe('.glide builder', () => {
    test('$(...).glide(g) builds a GlidePattern wrapping the inner pattern', () => {
        const r = patternPayload($('c4 e4 g4').glide(0.25));

        expect(isKind(r, 'GlidePattern')).toBe(true);
        expect(r.__kind).toBe('GlidePattern');
        expect(r.pattern.__kind).toBe('ParsedPattern');
        // A numeric glide is a raw constant, not a parsed pattern.
        expect(r.length).toBe(0.25);
    });

    test('a string glide parses as a mini-notation number pattern', () => {
        const r = patternPayload($('c4 e4 g4').glide('0 0.1 0.5'));
        if (typeof r.length === 'number') {
            throw new Error('expected a pattern glide, got a number');
        }
        expect(r.length.source).toBe('0 0.1 0.5');
        expect('Sequence' in r.length.ast).toBe(true);
    });

    test('only a string glide appends an argument span', () => {
        expect(
            patternPayload($('c4 e4').glide(0.1)).argument_spans,
        ).toHaveLength(1);
        expect(
            patternPayload($('c4 e4').glide('0 0.1')).argument_spans,
        ).toHaveLength(2);
        const chained = patternPayload(
            $('0 2 4').scale('c(maj)').trans('0 5').glide('0.1'),
        );
        expect(chained.argument_spans).toHaveLength(3);
    });

    test('curve and mode default to linear, in', () => {
        const r = patternPayload($('c4 e4').glide(0.25));
        expect(r.curve).toBe('linear');
        expect(r.mode).toBe('in');
    });

    test('carries an explicit curve and mode', () => {
        const r = patternPayload($('c4 e4').glide('0 0.25', 'expo', 'inOut'));
        expect(r.curve).toBe('expo');
        expect(r.mode).toBe('inOut');
        // Curve and mode are plain strings, never highlightable sources.
        expect(r.argument_spans).toHaveLength(2);
    });

    test('rejects unknown curves and modes', () => {
        const bad =
            (...args: unknown[]) =>
            () =>
                ($('c4').glide as (...a: unknown[]) => unknown)(...args);
        expect(bad(0.1, 'elastic')).toThrow(/curve must be one of/);
        expect(bad(0.1, 'quad', 'inout')).toThrow(/mode must be one of/);
        expect(bad(0.1, 'quad', 'sideways')).toThrow(MiniParseError);
    });

    test('rejects non-finite number args', () => {
        expect(() => $('c4').glide(NaN)).toThrow(MiniParseError);
        expect(() => $('c4').glide(Infinity)).toThrow(MiniParseError);
    });

    test('the wire payload is plain data', () => {
        const r = patternPayload($('c4').glide('0 0.1'));
        expect(Object.keys(r).sort()).toEqual([
            '__kind',
            'argument_spans',
            'curve',
            'length',
            'mode',
            'pattern',
        ]);
        expect(JSON.parse(JSON.stringify(r))).toEqual(r);
    });
});

describe('.glide composition', () => {
    test('chains with .fast/.beat in any order', () => {
        const r = patternPayload($('c4 e4').fast(2).glide(0.1).beat('0,2', 4));
        expect(isKind(r, 'BeatPattern')).toBe(true);
        expect(isKind(r.pattern, 'GlidePattern')).toBe(true);
        if (isKind(r.pattern, 'GlidePattern')) {
            expect(isKind(r.pattern.pattern, 'FastPattern')).toBe(true);
        }

        const r2 = patternPayload($('c4').glide('0 0.1').fast(2));
        expect(r2.__kind).toBe('FastPattern');
        expect(isKind(r2.pattern, 'GlidePattern')).toBe(true);
        // Inner pattern span + glide span carry through the outer wrapper.
        expect(r2.argument_spans).toHaveLength(2);
    });

    test('works on scale patterns and $arrange patterns, and as arrange sections', () => {
        expect(
            patternPayload($('0 2 4').scale('c(maj)').glide(0.1)).pattern
                .__kind,
        ).toBe('ScalePattern');
        expect(
            patternPayload($arrange([2, $('c4')], [2, $('e4')]).glide(0.1))
                .pattern.__kind,
        ).toBe('ArrangePattern');

        const r = patternPayload(
            $arrange([2, $('c4').glide('0.1')], [2, $('e4').glide(0.2)]),
        );
        expect(r.sections[0].pattern.__kind).toBe('GlidePattern');
        // (c4 + glide) + e4 = 3 flat spans.
        expect(r.argument_spans).toHaveLength(3);
    });
});
