/**
 * A scope draws both sides of each lane that may be stereo.
 */

import { describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';

const EXECUTION_OPTIONS = {
    sampleRate: 48_000,
    workspaceRoot: '/workspace',
} as const;

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
        expect(traces(`$stereo($saw('c3'), $saw('g3')).scope()`)).toEqual([
            '0:0',
            '0:1',
        ]);
    });

    test('a buffer reader draws what its buffer records', () => {
        expect(
            traces(
                `$delayRead($buffer($stereo($saw('c3'), $saw('g3')), 1), 0.1).scope()`,
            ),
        ).toEqual(['0:0', '0:1']);
    });

    test('a deferred lane draws what it was set to', () => {
        expect(
            traces(`const d = $deferred()\nd.scope()\nd.set($saw('c3'))`),
        ).toEqual(['0:0']);
        expect(
            traces(
                `const d = $deferred()\nd.scope()\nd.set($stereo($saw('c3'), $saw('g3')))`,
            ),
        ).toEqual(['0:0', '0:1']);
    });

    test('a mono lane draws one', () => {
        expect(traces(`$saw(['c3', 'e3']).scope()`)).toEqual(['0:0', '1:0']);
    });
});
