/**
 * A pattern is the `$cycle` that plays it: it builds the same graph as the
 * `$cycle` written out, through one sequencer however it is used.
 */

import { describe, expect, test } from 'vitest';
import type { PatchGraph } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';
import { $p } from '../miniNotation';
import { patternPayload } from '../patterns';
import { serializeForIPC } from '../../serializeForIPC';

const OPTIONS = { sampleRate: 48_000, workspaceRoot: '/workspace' } as const;

function exec(source: string) {
    return executePatchScript(source, schemas, OPTIONS);
}

function cycles(patch: PatchGraph) {
    return patch.modules.filter((m) => m.moduleType === '$cycle');
}

const SPAN_KEYS = new Set([
    '__argument_spans',
    'argument_span',
    'argument_spans',
]);

/**
 * The modules of a patch by id, without the spans that follow the source
 * text. A pattern's `$cycle` is made where the pattern is first used, so the
 * order of the modules differs from the written-out form.
 */
function graph(source: string): { id: string; moduleType: string }[] {
    const modules: { id: string; moduleType: string }[] = JSON.parse(
        JSON.stringify(exec(source).patch.modules, (key, value: unknown) =>
            SPAN_KEYS.has(key) ? undefined : value,
        ),
    );
    return modules.sort((a, b) => a.id.localeCompare(b.id));
}

describe('a pattern builds the graph of its $cycle', () => {
    test.each([
        [`$saw(SEQ).amp($perc(SEQ)).out()`],
        [`SEQ.saw().lpf('800hz').out()`],
        [`$sine(SEQ).amp($adsr(SEQ._gate)).amp(SEQ._velocity).out()`],
        [`$sine(SEQ[0]).out(); $sine([...SEQ]).out()`],
        [`$sine('c3').add(SEQ).mul(SEQ).out({ gain: SEQ, pan: SEQ })`],
        [`$c(SEQ, 'g4').saw().out()`],
        [`SEQ.scope(); SEQ.out(); SEQ.outMono(1)`],
        [`SEQ.slice(0, 1).saw().out(); SEQ.chunk(1)[0].saw().out()`],
        [`SEQ.pipe((s) => $saw(s)).out()`],
        [`$saw('c3').lpf(SEQ).wet(SEQ).out()`],
        [`const bus = $bus((m) => m.out()); SEQ.saw().send(bus, SEQ)`],
        [`$delay($saw(SEQ), 0.25, { feedback: SEQ }).out()`],
        [`$saw('c3').amp($g1(SEQ)).out()`],
        [`$setOutputGain(SEQ); $saw('c3').out()`],
        [`const d = $deferred(); $saw(d).out(); d.set(SEQ)`],
        [`$pWarp($sine('c3'), $phase.mirror(SEQ)).out()`],
        [`$mix([SEQ, 'g4']).out()`],
    ])('%s', (body) => {
        for (const pattern of [
            `$p('c3 e3 [g3, b3]')`,
            `$p.s('0 2 4', 'c(maj)').trans.squeeze('0 7').fast(2)`,
        ]) {
            const implicit = graph(
                `const SEQ = ${pattern}\n${body}`.replaceAll('SEQ', 'seq'),
            );
            const explicit = graph(
                `const SEQ = $cycle(${pattern})\n${body}`.replaceAll(
                    'SEQ',
                    'seq',
                ),
            );
            expect(implicit).toEqual(explicit);
            expect(
                implicit.filter((m) => m.moduleType === '$cycle'),
            ).toHaveLength(1);
        }
    });

    test('the sequencer highlights the pattern source it plays', () => {
        const source = `const seq = $p('c3 e3')\n$saw(seq).out()`;
        const [cycle] = cycles(exec(source).patch);
        const spans = (
            cycle.params as {
                __argument_spans: Record<
                    string,
                    { start: number; end: number }
                >;
            }
        ).__argument_spans;
        expect(source.slice(spans.pattern.start, spans.pattern.end)).toBe(
            `'c3 e3'`,
        );
    });

    test('each output resolution is recorded where the pattern is used', () => {
        const { outputResolutions } = exec(
            `const seq = $p('c e')\n$saw(seq).amp($perc(seq)).out()`,
        );
        expect(outputResolutions.map((r) => `${r.slot} ${r.output}`)).toEqual([
            'freq cv',
            'trigger trig',
        ]);
    });
});

describe('a pattern that is not played', () => {
    test.each([
        `$p('c e')`,
        `const p = $p('c e'); p.fast(2).slow(2).struct('x').beat(0, 4).glide(0.1)`,
        `const p = $p.s('0 2', 'c(maj)'); p.trans('1'); p.trans.squeeze('1')`,
        `$p.arrange([1, $p('c')])`,
        `console.log($p('c e').beat(0, 4), $p('c e').glide(0.1))`,
        `JSON.stringify($p('c e').glide(0.1)); Object.keys($p('c'))`,
        `const p = $p('c e'); p.then; p.constructor; p.nope; String(Symbol.iterator in p)`,
    ])('makes no $cycle: %s', (source) => {
        expect(cycles(exec(`${source}\n$sine('c3').out()`).patch)).toEqual([]);
    });

    test.each([
        [$p('c e'), ['__kind', 'ast', 'source', 'all_spans']],
        [
            $p.s('0 2', 'c(maj)').trans('1'),
            ['__kind', 'sources', 'scale', 'modes', 'argument_spans'],
        ],
        [$p.arrange([1, $p('c')]), ['__kind', 'sections']],
        [$p('c e').fast(2), ['__kind', 'pattern', 'factor']],
        [$p('c e').struct('x ~'), ['__kind', 'pattern', 'bool_pattern']],
        [$p('c').beat(0, 4), ['__kind', 'pattern', 't']],
        [$p('c e').glide(0.1), ['__kind', 'pattern', 'mode']],
    ])('keeps its wire fields out of reach: %#', (pattern, fields) => {
        const handle = pattern as unknown as Record<string, unknown>;
        const payload = patternPayload(pattern) as unknown as Record<
            string,
            unknown
        >;
        for (const field of fields) {
            expect(payload[field], field).toBeDefined();
            expect(handle[field], field).toBeUndefined();
            expect(field in handle, field).toBe(false);
        }
    });

    test('has no keys of its own and serializes as its wire payload', () => {
        const pattern = $p('c e').glide(0.1);
        expect(Object.keys(pattern)).toEqual([]);
        expect(JSON.parse(JSON.stringify(pattern))).toEqual(
            JSON.parse(JSON.stringify(patternPayload(pattern))),
        );
        expect(serializeForIPC(pattern)).toEqual({});
    });
});

describe('a pattern as a collection', () => {
    test('has the members of the sequencer it plays through', () => {
        const { patch } = exec(`
            const seq = $p('c3, e3, g3')
            if (seq.length !== 3) throw new Error('length ' + seq.length)
            if ([...seq].length !== 3) throw new Error('spread')
            if (!('out' in seq) || !('_gate' in seq)) throw new Error('in')
            if (seq._gate.length !== 3) throw new Error('gate width')
            let n = 0
            for (const voice of seq) n += voice.length
            if (n !== 3) throw new Error('iteration')
            $saw(seq[2]).out()
        `);
        expect(cycles(patch)).toHaveLength(1);
        const [saw] = patch.modules.filter((m) => m.moduleType === '$saw');
        expect(saw.params.freq).toMatchObject([{ channel: 2, port: 'cv' }]);
    });

    test('.add and .sub on a scale pattern are signal arithmetic', () => {
        const { patch } = exec(
            `$sine($p.s('0 2', 'c(maj)').add(1).sub('c3')).out()`,
        );
        expect(cycles(patch)[0].params.pattern).toMatchObject({ modes: [] });
        expect(
            patch.modules.filter((m) => m.moduleType === '_arithmetic'),
        ).toHaveLength(2);
    });

    test('a pattern method wins over a chain module of the same name', () => {
        const fast = $p('c e').fast(2);
        expect(patternPayload(fast)).toMatchObject({ __kind: 'FastPattern' });
    });

    test('an error in the sequencer points at the call that built the pattern', () => {
        expect(() =>
            exec(`const p = $p.s('0 2', 'c(bogus)')\n\n\n$sine(p).out()`),
        ).toThrow(/\$cycle at line 1\b/);
    });

    test('separately built patterns and .cycle() each get their own $cycle', () => {
        const { patch } = exec(`
            const p = $p('c e')
            $sine(p).out(); $sine(p).out()
            $sine(p.fast(2)).out(); $sine(p.fast(2)).out()
            $sine(p.cycle()).out(); $sine($cycle(p)).out()
        `);
        expect(cycles(patch)).toHaveLength(5);
    });
});
