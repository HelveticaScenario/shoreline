/**
 * Tests for the cycleToChannels() collection utility.
 */

import { beforeEach, describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { DSLContext } from '../factories';
import {
    type AnyCollection,
    type Collection,
    cycleToChannels,
    type GraphBuilder,
} from '../GraphBuilder';

let builder: GraphBuilder;

beforeEach(() => {
    builder = new DSLContext(schemas).getBuilder();
});

/** An N-channel signal (one $sine per channel). */
function sig(channels: number): Collection {
    const freqs = Array.from({ length: channels }, (_, i) => 1 + i);
    return builder.getFactory('$sine')(freqs) as Collection;
}

/** Stable per-channel identifiers for a signal. */
function ids(s: AnyCollection): string[] {
    return s.items.map((o) => o.toString());
}

describe('cycleToChannels()', () => {
    test('broadcasts a mono signal across N channels', () => {
        const mono = sig(1);
        const out = cycleToChannels(mono, 4);
        expect(out.length).toBe(4);
        // every channel references the one source channel
        expect(ids(out)).toEqual(Array(4).fill(ids(mono)[0]));
    });

    test('cycles a wider signal (channel i ← source i % len)', () => {
        const stereo = sig(2);
        const [a, b] = ids(stereo);
        expect(ids(cycleToChannels(stereo, 5))).toEqual([a, b, a, b, a]);
    });

    test('truncates to a narrower target', () => {
        const out = cycleToChannels(sig(4), 2);
        expect(out.length).toBe(2);
    });

    test('accepts one channel of a wider signal', () => {
        const stereo = sig(2);
        const [first] = stereo;
        const out = cycleToChannels(first, 3);
        expect(ids(out)).toEqual(Array(3).fill(ids(stereo)[0]));
    });
});
