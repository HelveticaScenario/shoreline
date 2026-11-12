import { isKind } from './kinds';
import { describe, expect, test } from 'vitest';

import { $p, MiniParseError } from '../index';
import { ALIGNMENT_MODES } from '../../alignmentModes';
import { patternPayload } from '../../patterns';

describe('$p.s builder', () => {
    test('returns an SpPattern wrapper with the wire-shape discriminator', () => {
        const r = patternPayload($p.s('0 2 4', 'c(major)'));
        expect(isKind(r, 'SpPattern')).toBe(true);
        expect(r.__kind).toBe('SpPattern');
        expect(r.scale).toBe('c(major)');
        expect(r.modes).toEqual([]);
        expect(r.sources.length).toBe(1);
        expect(r.sources[0].source).toBe('0 2 4');
    });

    test('non-string source throws', () => {
        expect(() => $p.s(123 as never, 'c(major)')).toThrow(MiniParseError);
    });

    test('non-string scale throws', () => {
        expect(() => $p.s('0', 123 as never)).toThrow(MiniParseError);
    });

    test('argument_spans length tracks sources length', () => {
        const r = patternPayload($p.s('0', 'c(major)'));
        expect(r.argument_spans.length).toBe(r.sources.length);
    });
});

describe('$p.s empty/whitespace source rejection', () => {
    test('empty source throws MiniParseError', () => {
        expect(() => $p.s('', 'c(maj)')).toThrow(MiniParseError);
    });

    test('whitespace-only source throws MiniParseError', () => {
        expect(() => $p.s('   ', 'c(maj)')).toThrow(MiniParseError);
    });

    test('empty chained .trans(rhs) throws MiniParseError', () => {
        expect(() => $p.s('0 2', 'c(maj)').trans('')).toThrow(MiniParseError);
    });

    test('whitespace-only chained .trans(rhs) throws MiniParseError', () => {
        expect(() => $p.s('0 2', 'c(maj)').trans('   ')).toThrow(
            MiniParseError,
        );
    });

    test('a rest "~" source does NOT throw — it is a real atom', () => {
        expect(() => $p.s('~', 'c(maj)')).not.toThrow();
    });
});

describe('$p.s chain methods', () => {
    test('.trans(rhs) appends a source with default mode "in"', () => {
        const r = patternPayload($p.s('0 2 4', 'c(maj)').trans('0 2'));
        expect(r.sources.length).toBe(2);
        expect(r.modes.length).toBe(1);
        expect(r.modes[0]).toEqual('in');
        expect(r.sources[1].source).toBe('0 2');
        expect(r.argument_spans.length).toBe(2);
    });

    test('bare .trans(x) is deep-equal to .trans.in(x)', () => {
        const a = patternPayload($p.s('0 2 4', 'c(maj)').trans('0 2'));
        const b = patternPayload($p.s('0 2 4', 'c(maj)').trans.in('0 2'));
        expect(a.modes).toEqual(b.modes);
        expect(a.sources[1].source).toBe(b.sources[1].source);
    });

    test('all 7 modes are callable on .trans', () => {
        const base = $p.s('0 1 2', 'c(maj)');
        for (const mode of ALIGNMENT_MODES) {
            const r = patternPayload(base.trans[mode]('0'));
            expect(r.modes[0]).toBe(mode);
        }
    });

    test('chain accumulates: .trans(...).trans.squeeze(...)', () => {
        const r = patternPayload(
            $p.s('0 2 4', 'c(maj)').trans('0 2').trans.squeeze('1'),
        );
        expect(r.sources.length).toBe(3);
        expect(r.modes.length).toBe(2);
        expect(r.modes[0]).toEqual('in');
        expect(r.modes[1]).toEqual('squeeze');
    });

    test('chain is immutable — original SpPattern unchanged', () => {
        const base = $p.s('0', 'c(maj)');
        const chained = patternPayload(base.trans('1'));
        expect(patternPayload(base).sources.length).toBe(1);
        expect(patternPayload(base).modes.length).toBe(0);
        expect(chained.sources.length).toBe(2);
    });

    test('non-string RHS throws MiniParseError', () => {
        const base = $p.s('0', 'c(maj)');
        expect(() => base.trans(123 as never)).toThrow(MiniParseError);
        expect(() => base.trans.squeeze(null as never)).toThrow(MiniParseError);
    });
});

describe('$p.s wire payload', () => {
    test('is plain data: kind, scale, sources, modes and spans', () => {
        const r = patternPayload($p.s('0 2', 'c(maj)').trans('1'));
        expect(JSON.parse(JSON.stringify(r))).toEqual(r);
        expect(r.__kind).toBe('SpPattern');
        expect(r.scale).toBe('c(maj)');
        expect(r.modes).toEqual(['in']);
        expect(r.sources.length).toBe(2);
        expect(r.argument_spans.length).toBe(2);
    });

    test('each source carries the standard ParsedPatternPayload fields', () => {
        const r = patternPayload($p.s('0 2 4', 'c(maj)').trans('0 2'));
        for (const s of r.sources) {
            expect(typeof s.source).toBe('string');
            expect(Array.isArray(s.all_spans)).toBe(true);
            // AST is the parsed MiniAST object.
            expect(typeof s.ast).toBe('object');
        }
    });
});

describe('$p.s source AST integrity', () => {
    test('rest atoms pass through into the source AST', () => {
        const r = patternPayload($p.s('0 ~ 4', 'c(maj)'));
        const ast = r.sources[0].ast;
        if ('Sequence' in ast) {
            expect('Rest' in ast.Sequence[1][0]).toBe(true);
        } else {
            throw new Error('Expected a sequence AST');
        }
    });

    test('chain RHS is parsed into its own AST', () => {
        const r = patternPayload($p.s('0 2 4', 'c(maj)').trans('1 3'));
        expect(r.sources[1].source).toBe('1 3');
        const ast = r.sources[1].ast;
        expect('Sequence' in ast).toBe(true);
    });

    test('euclidean modifier in source preserved', () => {
        const r = patternPayload($p.s('0(3,8)', 'c(maj)'));
        const ast = r.sources[0].ast;
        expect('Euclidean' in ast).toBe(true);
    });
});

describe('$p.s source string + spans', () => {
    test('source string is preserved verbatim', () => {
        const r = patternPayload($p.s('0 2 4', 'c(maj)'));
        expect(r.sources[0].source).toBe('0 2 4');
    });

    test('all_spans count matches the number of atoms', () => {
        const r = patternPayload($p.s('0 2 4', 'c(maj)'));
        expect(r.sources[0].all_spans.length).toBe(3);
    });
});
