import { isKind } from './kinds';
import { describe, expect, test } from 'vitest';

import { $p, MiniParseError } from '../index';
import { replaceSignals } from '../../GraphBuilder';
import { patternPayload } from '../../patterns';

describe('playing a pattern outside a patch run', () => {
    test('a collection member and .cycle() say a pattern is played only while a patch runs', () => {
        const pattern = $p('c4 e4') as unknown as { out(): unknown };
        expect(() => pattern.out()).toThrow(
            /only be played while a patch runs/,
        );
        expect(() => $p('c4 e4').cycle()).toThrow(
            /only be played while a patch runs/,
        );
    });
});

describe('.fast / .slow builders', () => {
    test('$p(...).fast(n) builds a FastPattern wrapping the inner pattern', () => {
        const r = patternPayload($p('c4 e4').fast(2));

        expect(isKind(r, 'FastPattern')).toBe(true);
        expect(r.__kind).toBe('FastPattern');
        expect(r.pattern.__kind).toBe('ParsedPattern');
        // A numeric factor is a raw constant, not a parsed pattern.
        expect(r.factor).toBe(2);
    });

    test('$p(...).slow(n) builds a SlowPattern', () => {
        const r = patternPayload($p('c4 e4').slow(2));
        expect(isKind(r, 'SlowPattern')).toBe(true);
        expect(r.__kind).toBe('SlowPattern');
        expect(r.pattern.__kind).toBe('ParsedPattern');
        expect(r.factor).toBe(2);
    });

    test('a string factor is parsed as a mini-notation number pattern', () => {
        const { factor } = patternPayload($p('c4').fast('2 4'));
        if (typeof factor === 'number') {
            throw new Error('expected a pattern factor, got a number');
        }
        expect(factor.source).toBe('2 4');
        // "2 4" is a two-step sequence, not a single Pure atom.
        expect('Sequence' in factor.ast).toBe(true);
    });

    test('.fast / .slow chain and nest', () => {
        const r = patternPayload($p('c4 e4').fast(3).slow(3));
        expect(r.__kind).toBe('SlowPattern');
        expect(isKind(r.pattern, 'FastPattern')).toBe(true);
        if (isKind(r.pattern, 'FastPattern')) {
            expect(r.pattern.pattern.__kind).toBe('ParsedPattern');
        }
    });

    test('works on $p.s and $p.arrange patterns', () => {
        const onSp = patternPayload($p.s('0 2 4', 'c(maj)').fast(2));
        expect(onSp.__kind).toBe('FastPattern');
        expect(onSp.pattern.__kind).toBe('SpPattern');

        const onArrange = patternPayload(
            $p.arrange([2, $p('c4')], [2, $p('e4')]).slow(2),
        );
        expect(onArrange.__kind).toBe('SlowPattern');
        expect(onArrange.pattern.__kind).toBe('ArrangePattern');
    });

    test('a number factor adds no argument span; a string factor adds one', () => {
        // Number factor: only the wrapped pattern's span(s). The bare number is
        // a constant and is not highlightable.
        expect(patternPayload($p('c4 e4').fast(2)).argument_spans).toHaveLength(
            1,
        );
        const chainedNum = patternPayload(
            $p.s('0 2 4', 'c(maj)').trans('0 5').fast(2),
        );
        expect(chainedNum.argument_spans).toHaveLength(2);
        // String factor: wrapped pattern's span(s) + the factor span.
        expect(
            patternPayload($p('c4 e4').fast('2 4')).argument_spans,
        ).toHaveLength(2);
        const chainedStr = patternPayload(
            $p.s('0 2 4', 'c(maj)').trans('0 5').fast('2 4'),
        );
        expect(chainedStr.argument_spans).toHaveLength(3);
    });

    test('a FastPattern is a valid $p.arrange section, spans flatten', () => {
        // Number factor → the FastPattern contributes only its inner span.
        const r = patternPayload(
            $p.arrange([2, $p('c4').fast(2)], [2, $p('e4')]),
        );
        expect(r.sections[0].pattern.__kind).toBe('FastPattern');
        // 1 (fast's inner c4) + 1 (e4) = 2 flat spans.
        expect(r.argument_spans).toHaveLength(2);
        // String factor → the FastPattern contributes inner + factor.
        const r2 = patternPayload(
            $p.arrange([2, $p('c4').fast('2 4')], [2, $p('e4')]),
        );
        // 2 (inner c4 + factor) + 1 (e4) = 3 flat spans.
        expect(r2.argument_spans).toHaveLength(3);
    });

    test('only the wire shape serializes', () => {
        const r = $p('c4').fast(2);
        expect(Object.keys(patternPayload(r)).sort()).toEqual([
            '__kind',
            'argument_spans',
            'factor',
            'pattern',
        ]);
        const json: { __kind: unknown; pattern: Record<string, unknown> } =
            JSON.parse(JSON.stringify(r));
        expect(json.__kind).toBe('FastPattern');
        expect('fast' in json).toBe(false);
        expect('slow' in json).toBe(false);
        // The nested pattern's methods are dropped too.
        expect('fast' in json.pattern).toBe(false);
    });

    test('survives replaceSignals verbatim (no AST collapse)', () => {
        const out = replaceSignals(patternPayload($p('c4 e4').fast('2 4')));
        expect(isKind(out, 'FastPattern')).toBe(true);
        if (!isKind(out, 'FastPattern')) return;
        expect(isKind(out.pattern, 'ParsedPattern')).toBe(true);
        if (isKind(out.pattern, 'ParsedPattern')) {
            expect(out.pattern.source).toBe('c4 e4');
        }
        const { factor } = out;
        if (typeof factor === 'number') {
            throw new Error('expected a pattern factor, got a number');
        }
        expect(factor.source).toBe('2 4');
    });

    test('a zero factor is allowed (lenient — lowers to silence in the engine)', () => {
        const r = patternPayload($p('c4 e4').fast(0));
        expect(r.__kind).toBe('FastPattern');
        expect(r.factor).toBe(0);
    });

    test('rejects a non-finite number factor', () => {
        // NaN / Infinity are valid `number`s but can't cross the JSON wire.
        expect(() => $p('c4').fast(NaN)).toThrow(MiniParseError);
        expect(() => $p('c4').fast(Infinity)).toThrow(MiniParseError);
        // .slow goes through the same factor builder.
        expect(() => $p('c4').slow(NaN)).toThrow(MiniParseError);
    });
});
