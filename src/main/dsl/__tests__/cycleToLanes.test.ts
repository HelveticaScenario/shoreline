/**
 * Tests for the cycleToLanes() collection utility.
 */

import { beforeEach, describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { DSLContext } from '../factories';
import {
    type AnyCollection,
    type Collection,
    cycleToLanes,
    type GraphBuilder,
} from '../GraphBuilder';

let builder: GraphBuilder;

beforeEach(() => {
    builder = new DSLContext(schemas).getBuilder();
});

/** An N-lane signal (one $sine per lane). */
function sig(lanes: number): Collection {
    const freqs = Array.from({ length: lanes }, (_, i) => 1 + i);
    return builder.getFactory('$sine')(freqs) as Collection;
}

/** Stable per-lane identifiers for a signal. */
function ids(s: AnyCollection): string[] {
    return s.items.map((o) => o.toString());
}

describe('cycleToLanes()', () => {
    test('broadcasts a mono signal across N lanes', () => {
        const mono = sig(1);
        const out = cycleToLanes(mono, 4);
        expect(out.length).toBe(4);
        // every lane references the one source lane
        expect(ids(out)).toEqual(Array(4).fill(ids(mono)[0]));
    });

    test('cycles a wider signal (lane i ← source i % len)', () => {
        const stereo = sig(2);
        const [a, b] = ids(stereo);
        expect(ids(cycleToLanes(stereo, 5))).toEqual([a, b, a, b, a]);
    });

    test('truncates to a narrower target', () => {
        const out = cycleToLanes(sig(4), 2);
        expect(out.length).toBe(2);
    });

    test('accepts one lane of a wider signal', () => {
        const stereo = sig(2);
        const [first] = stereo;
        const out = cycleToLanes(first, 3);
        expect(ids(out)).toEqual(Array(3).fill(ids(stereo)[0]));
    });
});
