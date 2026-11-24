/**
 * `.concat` joins two collections of one type: their lanes, and a
 * multi-output module's outputs output by output, so the result still stands
 * for whichever output fits the input it feeds. A plain signal follows every
 * output of such a module.
 */

import { describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';

const EXECUTION_OPTIONS = {
    sampleRate: 48_000,
    workspaceRoot: '/workspace',
} as const;

function run(source: string) {
    return executePatchScript(source, schemas, EXECUTION_OPTIONS);
}

type Cable = { module: string; port: string; lane?: number };

/** The cables feeding `param` of the first module of `moduleType`. */
function cables(source: string, moduleType: string, param: string): Cable[] {
    const module = run(source).patch.modules.find(
        (m) => m.moduleType === moduleType,
    );
    const value = (module!.params as Record<string, unknown>)[param];
    return (Array.isArray(value) ? value : [value]) as Cable[];
}

/** Each cable as `module:port:lane`, with modules numbered by first use. */
function wiring(list: Cable[]): string[] {
    const ids = new Map<string, number>();
    return list.map((c) => {
        if (!ids.has(c.module)) ids.set(c.module, ids.size);
        return `${ids.get(c.module)}:${c.port}:${c.lane ?? 0}`;
    });
}

describe('concat', () => {
    test('joins the lanes of two collections in order', () => {
        expect(
            wiring(
                cables(
                    `out: $mix($sine(['c', 'e']).concat($sine('g')))`,
                    '$mix',
                    'inputs',
                ).flat(),
            ),
        ).toEqual(['0:output:0', '0:output:1', '1:output:0']);
    });

    test('keeps every output of a multi-output module by name', () => {
        expect(
            wiring(
                cables(
                    `out: $mix($enosc('c3').concat($enosc('g3'))._root)`,
                    '$mix',
                    'inputs',
                ).flat(),
            ),
        ).toEqual(['0:root:0', '1:root:0']);
    });

    test('a joined module still takes the output that fits its input', () => {
        expect(
            wiring(
                cables(
                    `const seq = $('c e').concat($('g b'))
                     out: $perc(seq)`,
                    '$perc',
                    'trigger',
                ),
            ),
        ).toEqual(['0:trig:0', '1:trig:0']);
        expect(
            wiring(
                cables(
                    `out: $saw($enosc('c3').concat($enosc('g3')))`,
                    '$saw',
                    'freq',
                ),
            ),
        ).toEqual(['0:all:0', '1:all:0']);
    });

    test('a slice of a joined module keeps its outputs', () => {
        expect(
            wiring(
                cables(
                    `const seq = $('c e').concat($('g b'))
                     out: $perc(seq[1])`,
                    '$perc',
                    'trigger',
                ),
            ),
        ).toEqual(['0:trig:0']);
    });

    test('a plain signal follows every output of a module with outputs', () => {
        const joined = (output: string) =>
            wiring(
                cables(
                    `out: $mix($saw('c3').lpf('c5').concat($sine('c'))${output})`,
                    '$mix',
                    'inputs',
                ).flat(),
            );
        expect(joined('._12db')).toEqual(['0:12db:0', '1:output:0']);
        expect(joined('._24db')).toEqual(['0:24db:0', '1:output:0']);
    });

    test('a module with other outputs follows as the signal it stands for', () => {
        expect(
            wiring(
                cables(
                    `out: $mix($saw('c3').lpf('c5').concat($enosc('c3'))._24db)`,
                    '$mix',
                    'inputs',
                ).flat(),
            ),
        ).toEqual(['0:24db:0', '1:all:0']);
    });

    test('a plain signal joins a module with outputs as its default', () => {
        expect(
            wiring(
                cables(
                    `out: $mix($sine('c').concat($enosc('c3')))`,
                    '$mix',
                    'inputs',
                ).flat(),
            ),
        ).toEqual(['0:output:0', '1:all:0']);
    });

    test('joins two delays', () => {
        expect(
            wiring(
                cables(
                    `out: $mix($saw('c').delay(0.25).concat($saw('e').delay(0.5)))`,
                    '$mix',
                    'inputs',
                ).flat(),
            ),
        ).toEqual(['0:output:0', '1:output:0']);
    });

    test('a $trackTap joins its audio and its controls', () => {
        const { patch } = run(
            `const tap = $trackTap('a').concat($trackTap('b'))
             out: $mix(tap)
             out2$2m: $mix(tap.level)
             a: $saw('c')
             b: $saw('e')`,
        );
        const mixes = patch.modules.filter((m) => m.moduleType === '$mix');
        const lanes = mixes.map(
            (m) => (m.params as { inputs: unknown[] }).inputs.flat().length,
        );
        expect(lanes.slice(0, 2)).toEqual([2, 2]);
    });

    test('a ranged collection does not join one without a range', () => {
        expect(() =>
            run(`out: $sine('1hz').range(0, 5).concat($c($saw('c')))`),
        ).toThrow(/same type/);
    });
});
