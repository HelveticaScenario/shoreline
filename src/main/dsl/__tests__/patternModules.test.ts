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

function patchOf(source: string): PatchGraph {
    return exec(source).patch;
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
        [`$stereoMix($saw('c3'), { width: SEQ }).out()`],
        [`$step(['c4', 'g4'], SEQ).out()`],
        [`$sine('c3').outMono(1, SEQ)`],
        [`$signal(1).range(SEQ, 5, -5, 5).out()`],
        [`$quantizer($sine('c4'), SEQ).out()`],
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

    test.each([
        [`$p('c3 e3')`, { pattern: `'c3 e3'` }],
        [
            `$p('c3 e3').fast('2 4')`,
            { 'pattern.0': `'c3 e3'`, 'pattern.1': `'2 4'` },
        ],
        [
            `$p.s('0 2', 'c(maj)').trans('0 7')`,
            { 'pattern.0': `'0 2'`, 'pattern.1': `'0 7'` },
        ],
    ])(
        'the sequencer of %s highlights only its pattern sources',
        (pattern, texts) => {
            const source = `const seq = ${pattern}\n$saw(seq).out()`;
            const { patch, sourceLocationMap } = exec(source);
            const [cycle] = cycles(patch);
            const spans = (
                cycle.params as {
                    __argument_spans: Record<
                        string,
                        { start: number; end: number }
                    >;
                }
            ).__argument_spans;
            expect(
                Object.fromEntries(
                    Object.entries(spans).map(([key, { start, end }]) => [
                        key,
                        source.slice(start, end),
                    ]),
                ),
            ).toEqual(texts);
            expect(sourceLocationMap.get(cycle.id)?.line).toBe(1);
        },
    );

    test('each pattern built at one call site plays through its own $cycle', () => {
        const { patch } = exec(
            `for (const note of ['c3', 'e3', 'g3']) $saw($p(note)).out()`,
        );
        expect(cycles(patch)).toHaveLength(3);
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

describe('a pattern where a signal or a pattern belongs', () => {
    test.each([
        '$p("c4 e4")',
        '$p.s("0 2 4", "C(major)")',
        '$p.arrange([2, $p("c4")], [1, $p("e4")])',
        '$p("c4").fast(2)',
        '$p("c4").slow(2)',
        '$p("c4").struct("x ~")',
        '$p("c4").beat("0,7", 16)',
        '$p("c4 e4").glide(0.25)',
    ])('%s is its $cycle', (pattern) => {
        const patch = patchOf(
            `const p = ${pattern}; $sine(p).out(); p.mul(2).out(); p[0]; [...p]; p._gate`,
        );
        expect(cycles(patch).length).toBe(1);
    });

    test('a pattern passed to $cycle stays a pattern', () => {
        const patch = patchOf(
            'const p = $p("c4 e4"); $sine($cycle(p)).out(); $p.arrange([2, p]).out()',
        );
        const made = cycles(patch);
        expect(made.length).toBe(2);
        expect(made[0].params.pattern).toMatchObject({
            __kind: 'ParsedPattern',
        });
        expect(made[1].params.pattern).toMatchObject({
            __kind: 'ArrangePattern',
            sections: [{ pattern: { __kind: 'ParsedPattern' } }],
        });
    });

    test('a pattern kept between runs plays through a new $cycle each run', () => {
        const source = '$p.kept ??= $p("c4 e4"); $sine($p.kept).out()';
        patchOf(source);
        expect(cycles(exec(source).patch).length).toBe(1);
    });
});

describe('the payload a pattern sends the engine', () => {
    /** `value` without the spans that follow the source text. */
    const spanless = (value: unknown): unknown =>
        JSON.parse(
            JSON.stringify(value, (key, v: unknown) =>
                SPAN_KEYS.has(key) ? undefined : v,
            ),
        );

    test.each([
        [`$p('c ~ [e, g]')`, () => $p('c ~ [e, g]')],
        [
            `$p.s('0 ~ 2', 'c(maj)').trans.squeeze('0 7')`,
            () => $p.s('0 ~ 2', 'c(maj)').trans.squeeze('0 7'),
        ],
        [
            `$p.arrange([2, $p('c ~')], [1, $p('e').fast('2 4')])`,
            () => $p.arrange([2, $p('c ~')], [1, $p('e').fast('2 4')]),
        ],
        [
            `$p('c ~ e').slow(2).struct('x ~ x').beat('0,3', 8).glide('0 0.1')`,
            () =>
                $p('c ~ e')
                    .slow(2)
                    .struct('x ~ x')
                    .beat('0,3', 8)
                    .glide('0 0.1'),
        ],
    ])('%s arrives as built, nulls and all', (source, build) => {
        const built = spanless(patternPayload(build()));
        expect(JSON.stringify(built)).toContain('null');
        for (const use of [
            `${source}.saw().out()`,
            `$cycle(${source}).saw().out()`,
            `${source}.cycle().saw().out()`,
            `const d = $deferred(); $saw(d).out(); d.set(${source})`,
        ]) {
            const [cycle] = cycles(exec(use).patch);
            expect(spanless(cycle.params.pattern), use).toEqual(built);
        }
    });
});

describe('the modules a pattern feeds', () => {
    test('a chain call on a pattern keeps its own argument spans', () => {
        const source = `$p('c*4').tr808.kick({ decay: 2 }).out()`;
        const { patch } = exec(source);
        const [kick] = patch.modules.filter(
            (m) => m.moduleType === '$tr808.kick',
        );
        const { decay } = (
            kick.params as {
                __argument_spans: Record<
                    string,
                    { start: number; end: number }
                >;
            }
        ).__argument_spans;
        expect(source.slice(decay.start, decay.end)).toBe('2');
    });

    test('are attributed to their own call after a pattern fails to play', () => {
        const { patch, sourceLocationMap } = exec(
            `try { $sine($p.s('0', 'c(bogus)')) } catch {}\n\n$sine('c4').out()`,
        );
        const [sine] = patch.modules.filter((m) => m.moduleType === '$sine');
        expect(sourceLocationMap.get(sine.id)?.line).toBe(3);
    });

    test('a pattern in the options position is refused', () => {
        expect(() => exec(`$sine('c3', $p('0 1')).out()`)).toThrow(
            /\$sine takes 1 positional argument; the one after is its options/,
        );
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

    test('takes no properties of the script', () => {
        expect(() => exec(`const p = $p('c e'); p.fast = 2`)).toThrow();
        expect(() => exec(`const p = $p('c e'); p._gate = 2`)).toThrow();
    });

    test('has no keys of its own and gives its payload to no serializer', () => {
        const pattern = $p('c e').glide(0.1);
        expect(Object.keys(pattern)).toEqual([]);
        expect(JSON.stringify(pattern)).toBe('{}');
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
        expect(patch.modules.map((m) => m.moduleType)).toEqual(
            expect.arrayContaining(['$add', '$sub']),
        );
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
