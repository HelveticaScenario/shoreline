import { isKind } from './kinds';
import { describe, expect, test } from 'vitest';

import { $, $arrange, MiniParseError } from '../index';
import { patternPayload } from '../../patterns';

describe('$arrange builder', () => {
    test('builds an ArrangePattern wire object from [cycles, pattern] tuples', () => {
        const r = patternPayload(
            $arrange([2, $('c4 e4')], [4, $('0 2 4').scale('c(major)')]),
        );

        expect(isKind(r, 'ArrangePattern')).toBe(true);
        expect(r.__kind).toBe('ArrangePattern');
        expect(r.sections).toHaveLength(2);

        expect(r.sections[0].cycles).toBe(2);
        expect(r.sections[0].pattern.__kind).toBe('ParsedPattern');
        expect(r.sections[1].cycles).toBe(4);
        expect(r.sections[1].pattern.__kind).toBe('ScalePattern');

        // Flat argument_spans: one per source across sections, in order.
        expect(r.argument_spans).toHaveLength(2);
    });

    test('encodes Infinity as the string sentinel "Infinity"', () => {
        const r = patternPayload(
            $arrange([2, $('c4')], [Infinity, $('0 2 4').scale('a(min)')]),
        );
        expect(r.sections[0].cycles).toBe(2);
        expect(r.sections[1].cycles).toBe('Infinity');
    });

    test('nests: an ArrangePattern is a valid section, spans flatten', () => {
        const nested = $arrange([1, $('e4')], [1, $('g4')]);
        expect(patternPayload(nested).argument_spans).toHaveLength(2);

        const r = patternPayload($arrange([2, $('c4')], [2, nested]));
        expect(r.sections[1].pattern.__kind).toBe('ArrangePattern');
        // 1 (c4) + 2 (nested's two sources) = 3 flat spans.
        expect(r.argument_spans).toHaveLength(3);
    });

    test('flattens chained scale-pattern argument spans across sections', () => {
        // A chained scale pattern contributes one span per source in the chain.
        const chained = $('0 2 4').scale('c(maj)').trans('0 5');
        expect(patternPayload(chained).argument_spans).toHaveLength(2);

        const r = patternPayload($arrange([1, $('c4')], [1, chained]));
        // 1 (c4) + 2 (chain) = 3.
        expect(r.argument_spans).toHaveLength(3);
    });

    test('rejects an empty arrangement', () => {
        expect(() => ($arrange as () => unknown)()).toThrow(MiniParseError);
    });

    test('rejects non-integer, zero, and negative cycle counts', () => {
        for (const bad of [2.5, 0, -1]) {
            expect(() => $arrange([bad, $('c4')])).toThrow(MiniParseError);
        }
    });

    test('rejects a non-[cycles, pattern] section', () => {
        expect(() =>
            $arrange([2] as unknown as [number, ReturnType<typeof $>]),
        ).toThrow(MiniParseError);
    });

    test('rejects a non-pattern section payload', () => {
        expect(() =>
            $arrange([2, 'c4' as unknown as ReturnType<typeof $>]),
        ).toThrow(MiniParseError);
    });

    test('rejects an Infinity section that is not last', () => {
        expect(() => $arrange([Infinity, $('c4')], [2, $('e4')])).toThrow(
            MiniParseError,
        );
    });
});
