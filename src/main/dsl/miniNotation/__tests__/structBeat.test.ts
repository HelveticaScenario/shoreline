import { isKind } from './kinds';
import { describe, expect, test } from 'vitest';

import { $, $arrange, MiniParseError } from '../index';
import { patternPayload } from '../../patterns';

describe('.struct builder', () => {
    test('$(...).struct(s) builds a StructPattern wrapping the inner pattern', () => {
        const r = patternPayload($('c4 e4').struct('x ~ x x'));

        expect(isKind(r, 'StructPattern')).toBe(true);
        expect(r.__kind).toBe('StructPattern');
        expect(r.pattern.__kind).toBe('ParsedPattern');
        expect(r.bool_pattern.source).toBe('x ~ x x');
        // "x ~ x x" is a four-step sequence, not a single Pure atom.
        expect('Sequence' in r.bool_pattern.ast).toBe(true);
    });

    test('accepts numeric bool atoms and full mini-notation', () => {
        expect(patternPayload($('c4').struct('1 0 1 1')).__kind).toBe(
            'StructPattern',
        );
        expect(patternPayload($('c4').struct('x ~ <x ~>')).__kind).toBe(
            'StructPattern',
        );
        expect(patternPayload($('c4').struct('x(3,8)')).__kind).toBe(
            'StructPattern',
        );
    });

    test('rejects a non-string bool pattern', () => {
        // @ts-expect-error deliberate misuse
        expect(() => $('c4').struct(3)).toThrow(MiniParseError);
    });

    test('rejects an unparseable bool pattern', () => {
        // `x4` / `xx` are not atoms: `x` must stand alone.
        expect(() => $('c4').struct('x4')).toThrow(MiniParseError);
        expect(() => $('c4').struct('xx')).toThrow(MiniParseError);
    });

    test('the bool pattern always appends an argument span', () => {
        expect(
            patternPayload($('c4 e4').struct('x ~')).argument_spans,
        ).toHaveLength(2);
        const chained = patternPayload(
            $('0 2 4').scale('c(maj)').trans('0 5').struct('x ~'),
        );
        expect(chained.argument_spans).toHaveLength(3);
    });

    test('the wire payload is plain data', () => {
        const r = patternPayload($('c4').struct('x ~'));
        expect(Object.keys(r).sort()).toEqual([
            '__kind',
            'argument_spans',
            'bool_pattern',
            'pattern',
        ]);
        expect(JSON.parse(JSON.stringify(r))).toEqual(patternPayload(r));
    });
});

describe('.beat builder', () => {
    test('$(...).beat(t, div) builds a BeatPattern wrapping the inner pattern', () => {
        const r = patternPayload($('c2').beat('0,7,10', 16));

        expect(isKind(r, 'BeatPattern')).toBe(true);
        expect(r.__kind).toBe('BeatPattern');
        expect(r.pattern.__kind).toBe('ParsedPattern');
        if (typeof r.t === 'number') {
            throw new Error('expected a pattern t, got a number');
        }
        expect(r.t.source).toBe('0,7,10');
        // "0,7,10" is a comma stack — one onset per index.
        expect('Stack' in r.t.ast).toBe(true);
        // A numeric div is a raw constant, not a parsed pattern.
        expect(r.div).toBe(16);
    });

    test('t and div each accept a number or a mini-notation string', () => {
        const both = patternPayload($('c2').beat(0, 4));
        expect(both.t).toBe(0);
        expect(both.div).toBe(4);

        const divPattern = patternPayload($('c2').beat('0,7', '<16 8>'));
        if (typeof divPattern.div === 'number') {
            throw new Error('expected a pattern div, got a number');
        }
        expect(divPattern.div.source).toBe('<16 8>');
    });

    test('string args append argument spans in t-then-div order', () => {
        // Both numbers: only the wrapped pattern's span.
        expect(patternPayload($('c2').beat(0, 16)).argument_spans).toHaveLength(
            1,
        );
        // String t only.
        expect(
            patternPayload($('c2').beat('0,7', 16)).argument_spans,
        ).toHaveLength(2);
        // String t and div.
        expect(
            patternPayload($('c2').beat('0,7', '<16 8>')).argument_spans,
        ).toHaveLength(3);
    });

    test('rejects non-finite number args', () => {
        expect(() => $('c2').beat(NaN, 16)).toThrow(MiniParseError);
        expect(() => $('c2').beat(0, Infinity)).toThrow(MiniParseError);
    });

    test('the wire payload is plain data', () => {
        const r = patternPayload($('c2').beat('0,7', 16));
        expect(Object.keys(r).sort()).toEqual([
            '__kind',
            'argument_spans',
            'div',
            'pattern',
            't',
        ]);
        expect(JSON.parse(JSON.stringify(r))).toEqual(r);
    });
});

describe('.struct / .beat composition', () => {
    test('chain and nest with .fast/.slow in any order', () => {
        const r = patternPayload(
            $('c4 e4').fast(2).struct('x ~ x x').beat('0,2', 4),
        );
        expect(r.__kind).toBe('BeatPattern');
        expect(isKind(r.pattern, 'StructPattern')).toBe(true);
        if (isKind(r.pattern, 'StructPattern')) {
            expect(isKind(r.pattern.pattern, 'FastPattern')).toBe(true);
        }

        const r2 = patternPayload($('c4').struct('x ~').fast(2));
        expect(r2.__kind).toBe('FastPattern');
        expect(isKind(r2.pattern, 'StructPattern')).toBe(true);
    });

    test('work on scale patterns and $arrange patterns', () => {
        const onSp = patternPayload($('0 2 4').scale('c(maj)').struct('x ~ x'));
        expect(onSp.__kind).toBe('StructPattern');
        expect(onSp.pattern.__kind).toBe('ScalePattern');

        const onArrange = patternPayload(
            $arrange([2, $('c4')], [2, $('e4')]).beat(0, 4),
        );
        expect(onArrange.__kind).toBe('BeatPattern');
        expect(onArrange.pattern.__kind).toBe('ArrangePattern');
    });

    test('valid as $arrange sections, spans flatten', () => {
        const r = patternPayload(
            $arrange([2, $('c4').struct('x ~')], [2, $('e4').beat('0,7', 16)]),
        );
        expect(r.sections[0].pattern.__kind).toBe('StructPattern');
        expect(r.sections[1].pattern.__kind).toBe('BeatPattern');
        // (c4 + bool) + (e4 + t) = 4 flat spans.
        expect(r.argument_spans).toHaveLength(4);
    });
});
