/**
 * The run records each value that may be stereo where an input folds it to
 * mono, at the call and slot the editor finds it by, so the editor can show
 * `.mono()` after it.
 */

import { describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';

const EXECUTION_OPTIONS = {
    sampleRate: 48_000,
    workspaceRoot: '/workspace',
} as const;

/** The slots of the folds `source` records, in order. */
function folds(source: string): string[] {
    return executePatchScript(source, schemas, EXECUTION_OPTIONS).folds.map(
        (fold) => fold.slot,
    );
}

const WIDE = `const wide = $stereo($saw('c3'), $saw('g3'))\n`;

describe('folds', () => {
    test('a stereo signal into an input a stereo module folds', () => {
        expect(folds(`${WIDE}$enosc(wide).out()`)).toEqual(['pitch']);
        expect(
            folds(`${WIDE}$plate($saw('c3'), { decay: wide }).out()`),
        ).toEqual(['decay']);
    });

    test('an input read side by side does not fold', () => {
        expect(folds(`${WIDE}$plate(wide).out()`)).toEqual([]);
        expect(folds(`${WIDE}$mono(wide).out()`)).toEqual([]);
    });

    test('a lifted module carries stereo on', () => {
        expect(folds(`${WIDE}$lpf(wide, 'c5').out()`)).toEqual([]);
        expect(folds(`${WIDE}$enosc(wide.lpf('c5')).out()`)).toEqual(['pitch']);
    });

    test('a mono signal never folds', () => {
        expect(folds(`$enosc($saw('c3')).out()`)).toEqual([]);
        expect(folds(`$enosc('c3').out()`)).toEqual([]);
        expect(folds(`$saw('c3').outMono()`)).toEqual([]);
    });

    test('an array folds element by element', () => {
        expect(folds(`${WIDE}$enosc([wide, 'c3']).out()`)).toEqual(['pitch.0']);
    });

    test('a deferred signal may be stereo', () => {
        expect(
            folds(`const d = $deferred()\n$enosc(d).out()\nd.set('c3')`),
        ).toEqual(['pitch']);
    });

    test('a stereo signal sent to a mono out folds', () => {
        expect(folds(`${WIDE}wide.outMono(0)`)).toEqual(['receiver']);
    });

    test('a buffer reader follows its buffer', () => {
        expect(
            folds(`${WIDE}$enosc($delayRead($buffer(wide, 1), 0.1)).out()`),
        ).toEqual(['pitch']);
        expect(
            folds(`$enosc($delayRead($buffer($saw('c3'), 1), 0.1)).out()`),
        ).toEqual([]);
    });
});

describe('scopes', () => {
    /** Each scope lane of `source` as `lane:side`. */
    function traces(source: string): string[] {
        return executePatchScript(
            source,
            schemas,
            EXECUTION_OPTIONS,
        ).patch.scopes[0].lanes.map((lane) => `${lane.lane}:${lane.side ?? 0}`);
    }

    test('a stereo lane draws both sides', () => {
        expect(traces(`${WIDE}wide.scope()`)).toEqual(['0:0', '0:1']);
    });

    test('a mono lane draws one', () => {
        expect(traces(`$saw(['c3', 'e3']).scope()`)).toEqual(['0:0', '1:0']);
    });
});
