/**
 * Tests for BaseCollection indexing, iteration, slice and chunk.
 */

import { beforeAll, describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import {
    Collection,
    CollectionWithRange,
    DeferredCollection,
    DeferredModuleOutput,
    GraphBuilder,
    ModuleOutput,
} from '../GraphBuilder';
import { markRoot, outputSetOf } from '../outputResolution';

let builder: GraphBuilder;

beforeAll(() => {
    builder = new GraphBuilder(schemas);
});

function makeOutputs(n: number): ModuleOutput[] {
    return Array.from(
        { length: n },
        (_, i) => new ModuleOutput(builder, 'test-1', 'out', i),
    );
}

describe('BaseCollection.slice', () => {
    test('returns the same collection class with the selected items', () => {
        const outs = makeOutputs(5);
        const sliced = new Collection(...outs).slice(1, 3);
        expect(sliced).toBeInstanceOf(Collection);
        expect(sliced.items).toEqual([outs[1], outs[2]]);
        expect(sliced.length).toBe(2);
        expect(sliced[0].items).toEqual([outs[1]]);
    });

    test('follows Array.prototype.slice index semantics', () => {
        const outs = makeOutputs(5);
        const col = new Collection(...outs);
        expect(col.slice().items).toEqual(outs);
        expect(col.slice(3).items).toEqual(outs.slice(3));
        expect(col.slice(-2).items).toEqual(outs.slice(-2));
        expect(col.slice(1, -1).items).toEqual(outs.slice(1, -1));
        expect(col.slice(4, 2).length).toBe(0);
    });

    test('returns a new collection, leaving the original intact', () => {
        const outs = makeOutputs(3);
        const col = new Collection(...outs);
        const sliced = col.slice();
        expect(sliced).not.toBe(col);
        expect(col.items).toEqual(outs);
    });

    test('preserves CollectionWithRange and its ranges', () => {
        const ranged = new CollectionWithRange(
            ...makeOutputs(3).map((o, i) => o.withRange(i, i + 10)),
        );
        const sliced = ranged.slice(1);
        expect(sliced).toBeInstanceOf(CollectionWithRange);
        expect(sliced.items.map((o) => o.minValue)).toEqual([1, 2]);
        expect(sliced.items.map((o) => o.maxValue)).toEqual([11, 12]);
    });

    test('preserves DeferredCollection', () => {
        const deferred = new DeferredCollection(
            ...Array.from(
                { length: 3 },
                () => new DeferredModuleOutput(builder),
            ),
        );
        const sliced = deferred.slice(0, 2);
        expect(sliced).toBeInstanceOf(DeferredCollection);
        expect(sliced.items).toEqual(deferred.items.slice(0, 2));
    });
});

describe('BaseCollection indexing and iteration', () => {
    test('an index is a one-channel collection of that channel', () => {
        const outs = makeOutputs(3);
        const col = new Collection(...outs);
        expect(col[1]).toBeInstanceOf(Collection);
        expect(col[1].items).toEqual([outs[1]]);
        expect(col[1]).toBe(col[1]);
        expect(col[3]).toBeUndefined();
    });

    test('a one-channel collection is its own channel 0', () => {
        const col = new Collection(...makeOutputs(2));
        expect(col[0][0]).toBe(col[0]);
        expect([...col[0]]).toEqual([col[0]]);
    });

    test("a root's channels and slices keep every output's same channels", () => {
        const cv = new Collection(...makeOutputs(3));
        const gate = new Collection(
            ...Array.from(
                { length: 3 },
                (_, i) => new ModuleOutput(builder, 'test-1', 'gate', i),
            ),
        );
        const root = Object.assign(new Collection(...cv.items), {
            o: { cv, gate },
        });
        markRoot(root, { outputs: [], values: root.o });

        const [, second] = root;
        expect(second.items).toEqual([cv.items[1]]);
        const { o } = second as unknown as { o: typeof root.o };
        expect(o.gate.items).toEqual([gate.items[1]]);
        expect(outputSetOf(second)?.values).toBe(o);

        const tail = root.slice(1) as unknown as { o: typeof root.o };
        expect(tail.o.gate.items).toEqual(gate.items.slice(1));
    });

    test('a root output of another width stays whole in its slices', () => {
        const stereo = new Collection(...makeOutputs(2));
        const voices = new Collection(
            ...Array.from(
                { length: 4 },
                (_, i) => new ModuleOutput(builder, 'test-1', 'voices', i),
            ),
        );
        const root = Object.assign(new Collection(...stereo.items), {
            o: { stereo, voices },
        });
        markRoot(root, { outputs: [], values: root.o });

        const { o } = root[1] as unknown as { o: typeof root.o };
        expect(o.stereo.items).toEqual([stereo.items[1]]);
        expect(o.voices).toBe(voices);
    });

    test('a collection serializes to JSON', () => {
        const col = new Collection(...makeOutputs(1));
        expect(() => JSON.stringify(col)).not.toThrow();
    });

    test('iteration and spreading give the index views', () => {
        const col = new Collection(...makeOutputs(3));
        expect([...col]).toEqual([col[0], col[1], col[2]]);
        const [first, , third] = col;
        expect(first).toBe(col[0]);
        expect(third).toBe(col[2]);
    });

    test('a ranged channel keeps its range', () => {
        const ranged = new CollectionWithRange(
            ...makeOutputs(2).map((o, i) => o.withRange(i, i + 10)),
        );
        expect(ranged[1]).toBeInstanceOf(CollectionWithRange);
        expect(ranged[1].items[0].minValue).toBe(1);
        expect(ranged[1].items[0].maxValue).toBe(11);
    });

    test('a deferred channel sets its own placeholder', () => {
        const deferred = new DeferredCollection(
            ...Array.from(
                { length: 2 },
                () => new DeferredModuleOutput(builder),
            ),
        );
        const target = new ModuleOutput(builder, 'src-1', 'out', 0);
        deferred[1].set(new Collection(target));
        expect(deferred.items[1].resolve()).toBe(target);
        expect(deferred.items[0].resolve()).toBeNull();
    });
});

describe('BaseCollection.chunk', () => {
    test('splits into consecutive chunks with a shorter remainder last', () => {
        const outs = makeOutputs(5);
        const chunks = new Collection(...outs).chunk(2);
        expect(chunks.map((c) => c.items)).toEqual([
            [outs[0], outs[1]],
            [outs[2], outs[3]],
            [outs[4]],
        ]);
        for (const c of chunks) {
            expect(c).toBeInstanceOf(Collection);
        }
    });

    test('returns a plain array', () => {
        const chunks = new Collection(...makeOutputs(4)).chunk(2);
        expect(Array.isArray(chunks)).toBe(true);
    });

    test('a size at least the length yields one chunk of everything', () => {
        const outs = makeOutputs(3);
        const chunks = new Collection(...outs).chunk(10);
        expect(chunks.length).toBe(1);
        expect(chunks[0].items).toEqual(outs);
    });

    test('an empty collection yields no chunks', () => {
        expect(new Collection().chunk(2)).toEqual([]);
    });

    test('preserves CollectionWithRange', () => {
        const ranged = new CollectionWithRange(
            ...makeOutputs(4).map((o) => o.withRange(0, 5)),
        );
        for (const c of ranged.chunk(3)) {
            expect(c).toBeInstanceOf(CollectionWithRange);
        }
    });

    test.each([0, -1, 1.5, NaN])('rejects size %s', (size) => {
        expect(() => new Collection(...makeOutputs(2)).chunk(size)).toThrow(
            /positive integer/,
        );
    });
});
