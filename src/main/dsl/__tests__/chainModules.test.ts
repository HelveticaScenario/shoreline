/**
 * Chain modules: a module whose first argument is a signal is a method of
 * outputs and collections ($sine(0).lpf('100hz') ≡ $lpf($sine(0), '100hz')),
 * and `.wet(mix)` after one crossfades the receiver against the result.
 */

import { beforeEach, describe, expect, test } from 'vitest';
import { getReservedOutputNames, type PatchGraph } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import { DSLContext } from '../factories';
import {
    BaseCollection,
    Collection,
    CollectionWithRange,
    DeferredCollection,
    DeferredModuleOutput,
    type GraphBuilder,
    ModuleOutput,
    ModuleOutputWithRange,
} from '../GraphBuilder';
import { executePatchScript } from '../executor';
import { processModuleSchema, qualifiesForChain } from '../paramsSchema';

const EXECUTION_OPTIONS = {
    sampleRate: 48_000,
    workspaceRoot: '/workspace',
} as const;

function execPatch(source: string): PatchGraph {
    return executePatchScript(source, schemas, EXECUTION_OPTIONS).patch;
}

/** Recursively drop `__argument_spans` (which differ by source text, not graph). */
function stripSpans(value: unknown): unknown {
    if (Array.isArray(value)) {
        return value.map(stripSpans);
    }
    if (value && typeof value === 'object') {
        const out: Record<string, unknown> = {};
        for (const [k, v] of Object.entries(value)) {
            if (k === '__argument_spans') {
                continue;
            }
            out[k] = stripSpans(v);
        }
        return out;
    }
    return value;
}

/** Normalize a patch to its modules (id, type, span-free params), sorted by id. */
function normalize(patch: PatchGraph) {
    return patch.modules
        .map((m) => ({
            id: m.id,
            moduleType: m.moduleType,
            params: stripSpans(m.params),
        }))
        .sort((a, b) => a.id.localeCompare(b.id));
}

function moduleTypes(patch: PatchGraph): string[] {
    return patch.modules.map((m) => m.moduleType);
}

let builder: GraphBuilder;

beforeEach(() => {
    builder = new DSLContext(schemas).getBuilder();
});

type Chained = Record<string, (...args: unknown[]) => unknown>;

/** The chain path of every chainable module: its name without the `$`. */
const CHAIN_PATHS = schemas
    .filter((s) =>
        qualifiesForChain(
            processModuleSchema(
                s as unknown as Parameters<typeof processModuleSchema>[0],
            ),
        ),
    )
    .map((s) => s.name.slice(1));

describe('chain modules', () => {
    test('$sine(0).lpf("100hz") is identical to $lpf($sine(0), "100hz")', () => {
        const viaChain = execPatch('$sine(0).lpf("100hz").out()');
        const viaFactory = execPatch('$lpf($sine(0), "100hz").out()');
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
    });

    test('chains off a collection', () => {
        const viaChain = execPatch('$c($sine(0), $saw(1)).lpf("100hz").out()');
        const viaFactory = execPatch(
            '$lpf($c($sine(0), $saw(1)), "100hz").out()',
        );
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
    });

    test('multi-output module exposes its outputs on .o', () => {
        const sig = builder.getFactory('$sine')(0) as unknown as Chained;
        const xover = sig.xover() as { o: Record<string, unknown> };
        expect(Object.keys(xover.o)).toEqual(['thru', 'low', 'mid', 'high']);
    });

    test('chains off a named output and off a root', () => {
        const named = execPatch('$sine(0).xover().o.low.lpf("100hz").out()');
        const viaFactory = execPatch(
            '$lpf($xover($sine(0)).o.low, "100hz").out()',
        );
        expect(normalize(named)).toEqual(normalize(viaFactory));
        const root = execPatch('$sine(0).xover().lpf("100hz").out()');
        expect(normalize(root)).toEqual(
            normalize(execPatch('$lpf($xover($sine(0)), "100hz").out()')),
        );
    });

    test('a name that is no module reads as undefined', () => {
        const sig = builder.getFactory('$sine')(0) as unknown as Record<
            string,
            unknown
        >;
        expect(sig.notAModule).toBeUndefined();
        const namespace = sig.unstable as Record<PropertyKey, unknown>;
        expect(namespace.notAModule).toBeUndefined();
        expect(namespace[Symbol.iterator]).toBeUndefined();
        // `then` undefined keeps the namespace from being mistaken for a thenable.
        expect(namespace.then).toBeUndefined();
    });

    test('empty collection yields an empty Collection without throwing', () => {
        const empty = new Collection() as unknown as Chained & {
            unstable: { shape: Chained };
        };
        for (const result of [
            empty.lpf('100hz'),
            empty.unstable.shape.fold('dual', 2),
            (empty.lpf('100hz') as Collection).wet(2),
        ]) {
            expect(result).toBeInstanceOf(Collection);
            expect((result as Collection).items.length).toBe(0);
        }
    });

    test('every chainable module is a method of outputs and collections', () => {
        expect(CHAIN_PATHS.length).toBeGreaterThan(0);
        const sig = builder.getFactory('$sine')(0);
        for (const receiver of [sig, builder.$c(sig)]) {
            for (const path of CHAIN_PATHS) {
                const method = path
                    .split('.')
                    .reduce<unknown>(
                        (value, segment) =>
                            (value as Record<string, unknown>)[segment],
                        receiver,
                    );
                expect(method, path).toBeTypeOf('function');
            }
        }
    });
});

// ─── reserved names ──────────────────────────────────────────────────────────

describe('reserved names', () => {
    const reserved = new Set(getReservedOutputNames());

    test('no module name starts with a reserved word', () => {
        for (const { name } of schemas) {
            if (name.startsWith('_')) {
                continue;
            }
            expect(name.startsWith('$'), name).toBe(true);
            expect(reserved.has(name.slice(1).split('.')[0]), name).toBe(false);
        }
    });

    test('every built-in member of an output or collection is reserved', () => {
        const chained = new Set([
            ...CHAIN_PATHS.map((path) => path.split('.')[0]),
            'delay',
        ]);
        const sig = builder.getFactory('$sine')(0) as Collection;
        const members = [
            ModuleOutput,
            ModuleOutputWithRange,
            DeferredModuleOutput,
            BaseCollection,
            Collection,
            CollectionWithRange,
            DeferredCollection,
        ].flatMap((cls) => Object.getOwnPropertyNames(cls.prototype));
        // Fields are own properties of an instance, not of its prototype.
        const fields = [
            sig[0],
            sig.withRange(0, 1)[0],
            new DeferredModuleOutput(builder),
            sig,
        ].flatMap((instance) => Object.keys(instance));
        for (const member of [...members, ...fields]) {
            if (!chained.has(member) && !/^\d+$/.test(member)) {
                expect(reserved.has(member), member).toBe(true);
            }
        }
    });
});

// ─── `.wet` crossfade ────────────────────────────────────────────────────────

describe('.wet', () => {
    test('$saw(0).lpf("100hz").wet(2.5) equals .pipeMix(s => $lpf(s, "100hz"), 2.5)', () => {
        const viaWet = execPatch('$saw(0).lpf("100hz").wet(2.5).out()');
        const viaPipeMix = execPatch(
            '$saw(0).pipeMix((s) => $lpf(s, "100hz"), 2.5).out()',
        );
        expect(normalize(viaWet)).toEqual(normalize(viaPipeMix));
    });

    test('defaults to an equal mix', () => {
        expect(
            normalize(execPatch('$saw(0).lpf("100hz").wet().out()')),
        ).toEqual(normalize(execPatch('$saw(0).lpf("100hz").wet(2.5).out()')));
    });

    test('crossfade emits $mix, $remap, $clamp, and $scaleAndShift', () => {
        const types = moduleTypes(
            execPatch('$saw(0).lpf("100hz").wet(2.5).out()'),
        );
        expect(types).toContain('$mix');
        expect(types).toContain('$remap');
        expect(types).toContain('$clamp');
        expect(types).toContain('$scaleAndShift');
    });

    test('fades a collection against its chained result', () => {
        const viaWet = execPatch(
            '$c($saw(0), $saw(1)).lpf("100hz").wet(1).out()',
        );
        const viaPipeMix = execPatch(
            '$c($saw(0), $saw(1)).pipeMix((s) => $lpf(s, "100hz"), 1).out()',
        );
        expect(normalize(viaWet)).toEqual(normalize(viaPipeMix));
    });

    test('works on the root of a multi-output module and on a named output', () => {
        const root = execPatch('$saw(0).xover().wet(2).out()');
        expect(normalize(root)).toEqual(
            normalize(execPatch('$saw(0).pipeMix((s) => $xover(s), 2).out()')),
        );
        const named = execPatch('$saw(0).xover().o.low.wet(2).out()');
        expect(normalize(named)).toEqual(
            normalize(
                execPatch('$saw(0).pipeMix((s) => $xover(s).o.low, 2).out()'),
            ),
        );
    });

    test('fades against the receiver of the last chained module only', () => {
        const viaWet = execPatch(
            '$saw(0).lpf("100hz").hpf("50hz").wet(2).out()',
        );
        const viaPipeMix = execPatch(
            '$saw(0).lpf("100hz").pipeMix((s) => $hpf(s, "50hz"), 2).out()',
        );
        expect(normalize(viaWet)).toEqual(normalize(viaPipeMix));
    });

    test('throws on a signal that was not chained', () => {
        expect(() => execPatch('$saw(0).wet(2).out()')).toThrow(/\.wet\(\)/);
        expect(() =>
            execPatch('$saw(0).lpf("100hz").gain(2).wet(2).out()'),
        ).toThrow(/\.wet\(\)/);
    });
});

// ─── chain methods backed by a DSL helper ────────────────────────────────────

describe('$delay as a chain method', () => {
    test('$sine(0).delay(0.25) is identical to $delay($sine(0), 0.25)', () => {
        const viaChain = execPatch('$sine(0).delay(0.25).out()');
        const viaFactory = execPatch('$delay($sine(0), 0.25).out()');
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
    });

    test('.delay forwards options through to $delay', () => {
        const viaChain = execPatch(
            '$sine(0).delay(0.25, { feedback: 4, maxTime: 2 }).out()',
        );
        const viaFactory = execPatch(
            '$delay($sine(0), 0.25, { feedback: 4, maxTime: 2 }).out()',
        );
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
    });

    test('.delay(...).wet crossfades the dry signal against the wet delay', () => {
        const types = moduleTypes(
            execPatch('$sine(0).delay(0.25).wet(2.5).out()'),
        );
        expect(types).toContain('$delayRead'); // wet delay tap
        expect(types).toContain('$remap'); // crossfade dry leg
        expect(types).toContain('$scaleAndShift');
    });
});

// ─── nested `.unstable.shape.*` chain ──────────────────────────────────────

describe('namespaced chain modules', () => {
    test('.unstable.shape.<leaf> equals the bare $unstable.shape factory', () => {
        const viaChain = execPatch(
            "$sine(0).unstable.shape.fold('dual', 2).out()",
        );
        const viaFactory = execPatch(
            "$unstable.shape.fold($sine(0), 'dual', 2).out()",
        );
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
        expect(moduleTypes(viaChain)).toContain('$unstable.shape.fold');
    });

    test('leaf collisions are namespaced: $unstable.shape.fold coexists with $fold', () => {
        // `.unstable.shape.fold` resolves to the waveshaper…
        expect(
            moduleTypes(
                execPatch("$sine(0).unstable.shape.fold('dual', 2).out()"),
            ),
        ).toContain('$unstable.shape.fold');
        // …while the flat `.fold` still resolves to the fx wavefolder.
        expect(moduleTypes(execPatch('$sine(0).fold(2).out()'))).toContain(
            '$fold',
        );
    });

    test('.unstable.shape.<leaf>(...).wet crossfades dry against the shaped wet', () => {
        const types = moduleTypes(
            execPatch(
                "$sine(0).unstable.shape.saturate('hard', 3).wet(2.5).out()",
            ),
        );
        expect(types).toContain('$unstable.shape.saturate');
        expect(types).toContain('$remap'); // crossfade dry leg
    });
});

// ─── ModuleOutput.pipeMix crossfades ─────────────────────────────────────────

describe('ModuleOutput.pipeMix', () => {
    test('produces a $remap/$clamp crossfade, not a bare 2-input $mix', () => {
        const out = new ModuleOutput(builder, 'src-1', 'out', 0);
        out.pipeMix(
            (s) => builder.getFactory('$lpf')(s, '100hz') as Collection,
        );
        const types = builder.toPatch().modules.map((m) => m.moduleType);
        expect(types).toContain('$remap');
        expect(types).toContain('$clamp');
        expect(types).toContain('$mix');
    });
});
