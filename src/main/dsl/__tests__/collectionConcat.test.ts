/**
 * `.concat` joins two collections of one type: their lanes, and a
 * multi-output module's outputs output by output, so the result still stands
 * for whichever output fits the input it feeds.
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
                    `$mix($sine(['c', 'e']).concat($sine('g'))).out()`,
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
                    `$mix($enosc('c3').concat($enosc('g3'))._root).out()`,
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
                    `const seq = $p('c e').concat($p('g b'))
                     $perc(seq).out()`,
                    '$perc',
                    'trigger',
                ),
            ),
        ).toEqual(['0:trig:0', '1:trig:0']);
        expect(
            wiring(
                cables(
                    `$saw($enosc('c3').concat($enosc('g3'))).out()`,
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
                    `const seq = $p('c e').concat($p('g b'))
                     $perc(seq[1]).out()`,
                    '$perc',
                    'trigger',
                ),
            ),
        ).toEqual(['0:trig:0']);
    });

    test('a module with outputs does not join a plain signal', () => {
        expect(() => run(`$enosc('c3').concat($sine('c')).out()`)).toThrow(
            /same type/,
        );
    });

    test('a plain signal joins a module with outputs as its default', () => {
        expect(
            wiring(
                cables(
                    `$mix($sine('c').concat($enosc('c3'))).out()`,
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
                    `$mix($saw('c').delay(0.25).concat($saw('e').delay(0.5))).out()`,
                    '$mix',
                    'inputs',
                ).flat(),
            ),
        ).toEqual(['0:output:0', '1:output:0']);
    });

    test('a $trackTap joins its audio and its controls', () => {
        const { patch } = run(
            `const tap = $trackTap('a').concat($trackTap('b'))
             $mix(tap).out()
             $mix(tap.level).outMono(2)
             $saw('c').out({ label: 'a' })
             $saw('e').out({ label: 'b' })`,
        );
        const mixes = patch.modules.filter((m) => m.moduleType === '$mix');
        const lanes = mixes.map(
            (m) => (m.params as { inputs: unknown[] }).inputs.flat().length,
        );
        expect(lanes.slice(0, 2)).toEqual([4, 2]);
    });

    test('a ranged collection does not join one without a range', () => {
        expect(() =>
            run(`$sine('1hz').range(0, 5).concat($c($saw('c'))).out()`),
        ).toThrow(/same type/);
    });
});
