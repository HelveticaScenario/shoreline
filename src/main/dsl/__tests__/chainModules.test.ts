/**
 * Chain modules: a module whose first argument is a signal is a method of
 * outputs and collections ($sine(0).lpf('100hz') ≡ $lpf($sine(0), '100hz')),
 * and `.wet(mix)` after one crossfades the receiver against the result.
 */

import { beforeEach, describe, expect, test } from 'vitest';
import { getReservedMemberNames, type PatchGraph } from '@modular/core';
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
} from '../GraphBuilder';
import { executePatchScript } from '../executor';
import { dslModules } from '../../../shared/dsl/dslModuleDocs';
import { chainNames, functionName } from '../../../shared/dsl/referenceDocs';
import { buildSignalSiteTables } from '../signalSiteTables';
import { PATTERN_METHODS } from '../miniNotation';
import { TrackTap } from '../trackTap';
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
        const viaChain = execPatch('out: $sine(0).lpf("100hz")');
        const viaFactory = execPatch('out: $lpf($sine(0), "100hz")');
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
    });

    test('chains off a collection', () => {
        const viaChain = execPatch('out: $c($sine(0), $saw(1)).lpf("100hz")');
        const viaFactory = execPatch(
            'out: $lpf($c($sine(0), $saw(1)), "100hz")',
        );
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
    });

    test('multi-output module exposes its outputs by name', () => {
        const sig = builder.getFactory('$sine')(0) as unknown as Chained;
        const xover = sig.xover() as Record<string, unknown>;
        for (const name of ['_thru', '_low', '_mid', '_high']) {
            expect(xover[name]).toBeInstanceOf(BaseCollection);
        }
    });

    test('chains off a named output and off a root', () => {
        const named = execPatch('out: $sine(0).xover()._low.lpf("100hz")');
        const viaFactory = execPatch(
            'out: $lpf($xover($sine(0))._low, "100hz")',
        );
        expect(normalize(named)).toEqual(normalize(viaFactory));
        const root = execPatch('out: $sine(0).xover().lpf("100hz")');
        expect(normalize(root)).toEqual(
            normalize(execPatch('out: $lpf($xover($sine(0)), "100hz")')),
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

    test('every chainable module is a method of a collection', () => {
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

describe('modules the DSL builds', () => {
    const chained = chainNames(dslModules(schemas));

    test('every chainable one is a chain method of a signal', () => {
        const probes = chained
            .map(
                (name) =>
                    `if (typeof $sine(0).${name} !== 'function') throw new Error('${name}')`,
            )
            .join('\n');
        expect(() => execPatch(`${probes}\nout: $sine(0)`)).not.toThrow();
    });

    test('each has a signal site that chains as its docs say', () => {
        const { modules } = buildSignalSiteTables(schemas as never);
        for (const fn of dslModules(schemas)) {
            const name = functionName(fn);
            expect(modules[name]?.chainable, name).toBe(Boolean(fn.chainable));
        }
    });
});

// ─── reserved names ──────────────────────────────────────────────────────────

describe('reserved names', () => {
    const reserved = new Set(getReservedMemberNames());

    test('no module name starts with a reserved word', () => {
        for (const { name } of schemas) {
            // `$cycle` is what a pattern's reserved `.cycle(...)` calls.
            if (name.startsWith('_') || name === '$cycle') {
                continue;
            }
            expect(name.startsWith('$'), name).toBe(true);
            expect(reserved.has(name.slice(1).split('.')[0]), name).toBe(false);
        }
    });

    test('every built-in member of a collection is reserved', () => {
        const chained = new Set([
            ...CHAIN_PATHS.map((path) => path.split('.')[0]),
            ...chainNames(dslModules(schemas)),
        ]);
        const membersOf = (classes: { prototype: object }[]) =>
            classes.flatMap((cls) => Object.getOwnPropertyNames(cls.prototype));
        // The chain methods themselves sit on the base prototype.
        const base = membersOf([BaseCollection]).filter(
            (member) => !chained.has(member),
        );
        const subclasses = membersOf([
            Collection,
            CollectionWithRange,
            DeferredCollection,
            TrackTap,
        ]);
        // Fields are own properties of an instance, not of its prototype.
        const sig = builder.getFactory('$sine')(0) as Collection;
        const fields = [
            sig,
            sig.range(0, 1, -1, 1),
            new DeferredCollection(new DeferredModuleOutput(builder)),
        ]
            .flatMap((instance) => Object.keys(instance))
            .filter((member) => !/^\d+$/.test(member));
        expect(
            [...base, ...subclasses, ...fields].filter(
                (member) => !reserved.has(member),
            ),
        ).toEqual([]);
    });

    test('every pattern method is reserved, and no collection member shadows one', () => {
        for (const method of PATTERN_METHODS) {
            expect(reserved.has(method), method).toBe(true);
            expect(method in Collection.prototype, method).toBe(false);
            expect(method in CollectionWithRange.prototype, method).toBe(false);
        }
    });
});

// ─── `.wet` crossfade ────────────────────────────────────────────────────────

describe('.wet', () => {
    test('$saw(0).lpf("100hz").wet(2.5) equals .pipeMix(s => $lpf(s, "100hz"), 2.5)', () => {
        const viaWet = execPatch('out: $saw(0).lpf("100hz").wet(2.5)');
        const viaPipeMix = execPatch(
            'out: $saw(0).pipeMix((s) => $lpf(s, "100hz"), 2.5)',
        );
        expect(normalize(viaWet)).toEqual(normalize(viaPipeMix));
    });

    test('defaults to an equal mix', () => {
        expect(normalize(execPatch('out: $saw(0).lpf("100hz").wet()'))).toEqual(
            normalize(execPatch('out: $saw(0).lpf("100hz").wet(2.5)')),
        );
    });

    test('crossfade emits one $crossfade', () => {
        const types = moduleTypes(
            execPatch('out: $saw(0).lpf("100hz").wet(2.5)'),
        );
        expect(types.filter((t) => t === '$crossfade')).toHaveLength(1);
    });

    test('fades a collection against its chained result', () => {
        const viaWet = execPatch(
            'out: $c($saw(0), $saw(1)).lpf("100hz").wet(1)',
        );
        const viaPipeMix = execPatch(
            'out: $c($saw(0), $saw(1)).pipeMix((s) => $lpf(s, "100hz"), 1)',
        );
        expect(normalize(viaWet)).toEqual(normalize(viaPipeMix));
    });

    test('works on the root of a multi-output module and on a named output', () => {
        const root = execPatch('out: $saw(0).xover().wet(2)');
        expect(normalize(root)).toEqual(
            normalize(execPatch('out: $saw(0).pipeMix((s) => $xover(s), 2)')),
        );
        const named = execPatch('out: $saw(0).xover()._low.wet(2)');
        expect(normalize(named)).toEqual(
            normalize(
                execPatch('out: $saw(0).pipeMix((s) => $xover(s)._low, 2)'),
            ),
        );
    });

    test('works through a scope on a multi-output module', () => {
        expect(
            normalize(execPatch('out: $saw(0).xover().scope().wet(2)')),
        ).toEqual(normalize(execPatch('out: $saw(0).xover().wet(2)')));
        expect(
            normalize(execPatch('out: $saw(0).xover().scope()._low.wet(2)')),
        ).toEqual(normalize(execPatch('out: $saw(0).xover()._low.wet(2)')));
    });

    test('fades against the receiver of the last chained module only', () => {
        const viaWet = execPatch(
            'out: $saw(0).lpf("100hz").hpf("50hz").wet(2)',
        );
        const viaPipeMix = execPatch(
            'out: $saw(0).lpf("100hz").pipeMix((s) => $hpf(s, "50hz"), 2)',
        );
        expect(normalize(viaWet)).toEqual(normalize(viaPipeMix));
    });

    test('fades operator modules like any chained module', () => {
        for (const method of ['amp', 'gain', 'add', 'mul']) {
            const viaWet = execPatch(
                `out: $saw(0).lpf("100hz").${method}(2).wet(2)`,
            );
            const viaPipeMix = execPatch(
                `out: $saw(0).lpf("100hz").pipeMix((s) => $${method}(s, 2), 2)`,
            );
            expect(normalize(viaWet)).toEqual(normalize(viaPipeMix));
        }
    });

    test('blends a rounding against the signal it rounds', () => {
        const patch = execPatch('out: $saw(0).lpf("100hz").round().wet(2)');
        const crossfade = patch.modules.find(
            (m) => m.moduleType === '$crossfade',
        )!;
        const round = patch.modules.find((m) => m.moduleType === '$round')!;
        const lpf = patch.modules.find((m) => m.moduleType === '$lpf')!;
        const params = JSON.stringify(crossfade.params);
        expect(params).toContain(round.id);
        expect(params).toContain(lpf.id);
    });

    test('sums the dry voices once against a stereo effect', () => {
        // The effect hears every voice in its one stereo lane, so the dry
        // joins it as one lane: the wet is heard once, not once per voice.
        const patch = execPatch("out: $saw('c3:m7').plate().wet(2)");
        const saw = patch.modules.find((m) => m.moduleType === '$saw')!;
        const plate = patch.modules.find((m) => m.moduleType === '$plate')!;
        const voices = patch.modules.find(
            (m) =>
                m.moduleType === '$mix' &&
                JSON.stringify(m.params.inputs).includes(saw.id),
        )!;
        const inputs = voices.params.inputs as { module: string }[];
        expect(inputs).toHaveLength(4);
        expect(inputs.every((cable) => cable.module === saw.id)).toBe(true);
        const crossfade = patch.modules.find(
            (m) => m.moduleType === '$crossfade',
        )!;
        expect(crossfade.params.a).toEqual([
            { lane: 0, module: voices.id, port: 'output', type: 'cable' },
        ]);
        expect(JSON.stringify(crossfade.params.b)).toContain(plate.id);
    });

    test('fades one voice against a stereo effect as it is', () => {
        const patch = execPatch("out: $saw('c3').plate().wet(2)");
        const saw = patch.modules.find((m) => m.moduleType === '$saw')!;
        const crossfade = patch.modules.find(
            (m) => m.moduleType === '$crossfade',
        )!;
        expect(crossfade.params.a).toEqual([
            { lane: 0, module: saw.id, port: 'output', type: 'cable' },
        ]);
    });

    test('keeps each voice against an effect with a lane per voice', () => {
        const patch = execPatch("out: $saw('c3:m7').lpf('800hz').wet(2)");
        const saw = patch.modules.find((m) => m.moduleType === '$saw')!;
        const crossfade = patch.modules.find(
            (m) => m.moduleType === '$crossfade',
        )!;
        expect(JSON.stringify(crossfade.params.a)).toContain(saw.id);
    });

    test('throws on a signal that was not chained', () => {
        expect(() => execPatch('out: $saw(0).wet(2)')).toThrow(/\.wet\(\)/);
        expect(() =>
            execPatch('out: $saw(0).lpf("100hz").range(0, 1, -5, 5).wet(2)'),
        ).toThrow(/\.wet\(\)/);
    });
});

// ─── chain methods backed by a DSL helper ────────────────────────────────────

describe('$delay as a chain method', () => {
    test('$sine(0).delay(0.25) is identical to $delay($sine(0), 0.25)', () => {
        const viaChain = execPatch('out: $sine(0).delay(0.25)');
        const viaFactory = execPatch('out: $delay($sine(0), 0.25)');
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
    });

    test('.delay forwards options through to $delay', () => {
        const viaChain = execPatch(
            'out: $sine(0).delay(0.25, { feedback: 4, maxTime: 2 })',
        );
        const viaFactory = execPatch(
            'out: $delay($sine(0), 0.25, { feedback: 4, maxTime: 2 })',
        );
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
    });

    test('.delay(...).wet crossfades the dry signal against the wet delay', () => {
        const types = moduleTypes(
            execPatch('out: $sine(0).delay(0.25).wet(2.5)'),
        );
        expect(types).toContain('$delayRead'); // wet delay tap
        expect(types).toContain('$crossfade');
    });
});

// ─── nested `.unstable.shape.*` chain ──────────────────────────────────────

describe('namespaced chain modules', () => {
    test('.unstable.shape.<leaf> equals the bare $unstable.shape factory', () => {
        const viaChain = execPatch(
            "out: $sine(0).unstable.shape.fold('dual', 2)",
        );
        const viaFactory = execPatch(
            "out: $unstable.shape.fold($sine(0), 'dual', 2)",
        );
        expect(normalize(viaChain)).toEqual(normalize(viaFactory));
        expect(moduleTypes(viaChain)).toContain('$unstable.shape.fold');
    });

    test('leaf collisions are namespaced: $unstable.shape.fold coexists with $fold', () => {
        // `.unstable.shape.fold` resolves to the waveshaper…
        expect(
            moduleTypes(
                execPatch("out: $sine(0).unstable.shape.fold('dual', 2)"),
            ),
        ).toContain('$unstable.shape.fold');
        // …while the flat `.fold` still resolves to the fx wavefolder.
        expect(moduleTypes(execPatch('out: $sine(0).fold(2)'))).toContain(
            '$fold',
        );
    });

    test('.unstable.shape.<leaf>(...).wet crossfades dry against the shaped wet', () => {
        const types = moduleTypes(
            execPatch(
                "out: $sine(0).unstable.shape.saturate('hard', 3).wet(2.5)",
            ),
        );
        expect(types).toContain('$unstable.shape.saturate');
        expect(types).toContain('$crossfade');
    });
});

// ─── pipeMix crossfades dry against wet ──────────────────────────────────────

describe('pipeMix', () => {
    test('produces a $crossfade, not a bare 2-input $mix', () => {
        const out = new Collection(
            new ModuleOutput(builder, 'src-1', 'out', 0),
        );
        out.pipeMix(
            (s) => builder.getFactory('$lpf')(s, '100hz') as Collection,
        );
        const types = builder.toPatch().modules.map((m) => m.moduleType);
        expect(types).toContain('$crossfade');
        expect(types).not.toContain('$mix');
    });
});
